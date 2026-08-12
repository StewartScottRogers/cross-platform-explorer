---
id: CPE-1647
title: "vault_unlock takes session_dir straight off the IPC boundary with no containment check — so vault_lock can shred an arbitrary directory"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent Reviewer/Security lens on CPE-1630 (PR #836), while verifying that PR's claim that
`wipe_session_dir` was safe to leave ungated. The claim rests on an assumption the backend does not enforce.

**Pre-existing** — it predates CPE-1611 and CPE-1630 and was neither caused nor worsened by them.

## The gap
`vault_unlock` (`src-tauri/src/lib.rs:7003-7018`) takes `session_dir: String` **directly off the IPC
boundary with zero backend validation** that it resolves under the app's own `vault-sessions` cache root.
That containment exists only by frontend convention — `vaultStore.ts:114` builds
`join(base, "vault-sessions", crypto.randomUUID())`. Nothing in `unlock_to_session` or
`VaultRegistry::unlock_with_wiper` (`vault_manager.rs:249-256, 364-390`) checks the path at all.

The contrast is instructive: `create_vault` in the very same file **does** have exactly this kind of guard —
`resolves_inside` (`vault_manager.rs:222-240`) — to stop the destination blob landing inside the folder being
shredded. The pattern is established; it just isn't applied here.

**Consequence.** A devtools or automation caller holding a valid `.cpevault` blob and its passphrase can:

    vaultUnlock(blob, passphrase, "C:\Users\<you>\Documents")   // decrypts the vault's plaintext INTO Documents
    vaultLock(blob)                                             // -> wipe_session_dir -> shred_tree on Documents

and **every file under that directory is securely shredded** — including files that predate the vault and
have nothing to do with it — then the directory is removed. No confirmation of any kind, unlike `shred_paths`
(CPE-1611) and `vault_create` (CPE-1630) after this sprint.

So the blast radius is far larger than "wipes app-owned scratch data", which is how the exemption was
justified.

## What is NOT the fix
Adding a `confirmed` flag. Locking genuinely *should* always wipe its session directory — that is the
feature. The problem is that the directory being wiped is caller-chosen and unvalidated.

## Fix
Canonicalize `session_dir` in `unlock_to_session` and assert it resolves **under the app's `vault-sessions`
cache root**, mirroring `create_vault`'s existing `resolves_inside` guard. Refuse cleanly otherwise, with the
same error shape CPE-1599/1611/1630 established so all four refusals read alike.

Points to get right:
- **Canonicalize before comparing** — a path-prefix string test is defeated by `..`, by a symlink or junction
  in the session path, and by the `Photos` vs `Photos2` boundary problem. CPE-1613/CPE-1623 fought exactly
  this; reuse `path_key`-style identity comparison rather than inventing a third approach, and **fail closed**
  on any resolution failure.
- The session root is app-owned, so a strict containment check should have no legitimate false positives —
  verify that by running the real unlock/lock round-trip.

Also worth doing here: the orphan sweep (`sweep_orphan_sessions`, `vault_manager.rs:289-291`) was checked and
is **safe** — it only reads immediate children of a hardcoded `sessions_root` and does not walk above it. Say
so in the work log so the next auditor doesn't re-derive it.

## Acceptance criteria
- `vault_unlock` refuses a `session_dir` outside the app's session root; a test proves an arbitrary directory
  (with pre-existing files in it) is neither unlocked into nor shredded, verified by reading bytes off disk.
- The refusal is cleanly distinct and matches the CPE-1599/1611/1630 message shape.
- Symlink/junction/`..` variants of an out-of-root path are refused; every resolution-failure branch fails
  closed. Negative control per case.
- The normal unlock → edit → lock round-trip is unaffected.

**Conflict surface:** `crates/server/src/vault_manager.rs` (`unlock_to_session`, `unlock_with_wiper`),
`src-tauri/src/lib.rs` (`vault_unlock`), plus tests. Overlaps **CPE-1645** (locking silently destroys edits
made while unlocked) — both concern what `lock` does to the session directory, so design them together.

## Work Log

**2026-08-11 — fixed on branch `cpe-1647-vault-session-containment`.**

What was done:

- **Engine guard (`crates/server/src/vault_manager.rs`).** New `ensure_session_dir_contained(sessions_root,
  session_dir)` refuses any `session_dir` that does not resolve **strictly inside** the app's own
  `vault-sessions` root. `unlock_to_session` now takes `sessions_root` and runs that check **first** —
  before the blob is even read, before any decrypt, and long before any wipe — so a refused call writes
  nothing anywhere and records no mapping for a later `lock` to shred. `VaultRegistry::unlock` /
  `unlock_with_wiper` thread the root through. Enforcement is in the **engine**, not the adapter, so no
  future caller can reintroduce the hole by forgetting a check at its own boundary.
- **Resolution is canonical, not textual.** A new `resolve_for_containment` canonicalizes the nearest
  *existing* ancestor (resolving symlinks/junctions/`..`/`.`/verbatim quirks via the OS) and re-appends the
  not-yet-existing tail — a fresh session dir never exists yet. The tail is collected only via
  `Path::file_name`, which yields `None` for `..`/root/prefix, so a climb through non-existent components
  (`<root>/<uuid>/../../Documents`) is **rejected**, not optimistically "resolved". Comparison is
  `Path::starts_with` (component-wise), which is why `vault-sessions-evil` does not match `vault-sessions`.
  `session_dir == root` is refused too — wiping the root would shred every other live session.
- **Fails closed everywhere**: an uncreatable/unresolvable root, an unresolvable session path, and every
  escape variant all return a clean `VaultError::Format` in the CPE-1599/1611/1630 house style
  ("refusing to unlock: …"), never a panic and never a silent pass. The message names only the
  caller-supplied path (the caller already knows it) — it never echoes the resolved app/home root.
- **Adapter (`src-tauri/src/lib.rs`).** `vault_unlock` takes an injected `tauri::AppHandle` and resolves the
  root through a new shared `vault_sessions_root(app)` helper (`appCacheDir()/vault-sessions`), which the
  startup orphan sweep now also uses — one resolver, so the guard, the sweep, and the frontend's
  `defaultAllocSessionDir` can never name different directories. `AppHandle` is excluded from specta
  bindings, so `vaultUnlock`'s TS signature is unchanged and no frontend change was needed.
- **Explicitly NOT done:** no `confirmed` flag was added. Locking *should* always wipe its session dir;
  the bug was that the directory was caller-chosen and unvalidated. No scope creep into CPE-1645/CPE-1646 —
  `lock`'s semantics are untouched, so both slot in cleanly on top.

Audit note for the next reviewer (as the ticket asked): **`sweep_orphan_sessions` was re-checked and is
safe.** It reads only the immediate children of the hardcoded `sessions_root` and never walks above it; the
only change here is that its root now comes from the shared `vault_sessions_root` helper.

Test evidence (`crates/server/src/vault_manager.rs`, 6 new tests; expectations derived from the ticket's
threat model, and every "not destroyed" claim verified by reading bytes back **off disk**):

- `unlock_refuses_an_out_of_root_session_dir_and_lock_never_shreds_it` — the exact attack: unlock into a
  `Documents`-like dir holding pre-existing files, then `lock`. Refused; files byte-identical after **both**
  calls; no vault plaintext extracted; no registry state recorded.
- `unlock_refuses_dot_dot_traversal_out_of_the_session_root` — `<root>/../Outside` and
  `<root>/<uuid>/../../Outside` (the climb through a non-existent component).
- `unlock_refuses_a_session_dir_that_symlinks_out_of_the_root` — a real directory symlink inside the root,
  plus a fresh child under that symlinked ancestor. Skips **loudly** (`eprintln!("SKIPPED …")`) if the OS
  refuses to create the link; on this Windows box it did **not** skip — verified with `--nocapture`.
- `unlock_refuses_the_root_itself_a_prefix_sibling_and_an_unresolvable_root` — root itself (another vault's
  live session left intact), `vault-sessions-evil` prefix sibling, and a root whose parent is a regular file.
- `unlock_to_session_itself_refuses_an_out_of_root_target_before_reading_the_blob` — engine-level, and pins
  the ordering (the blob path does not exist, so a post-read check would surface an I/O error instead).
- `a_legitimate_fresh_session_dir_still_unlocks_and_is_still_wiped_on_lock` — **negative control**: a UUID
  child of a root that does not exist yet (the first-ever unlock) still unlocks, round-trips byte-identical,
  and is still securely wiped on lock, with the root itself surviving. "Refuse everything" cannot pass this.

Falsifiability probe: with the single `ensure_session_dir_contained(...)?` line temporarily removed, exactly
the 5 security tests **FAILED** and the negative control passed (24 passed / 5 failed); restored → 29/29.

Verification (all run synchronously, Windows):
- `cargo test` (crates/server) — **1927 + 21 + 22 + 45 + 32 + … passed, 0 failed** (2 pre-existing ignored).
- `cargo clippy --all-targets -- -D warnings` (crates/server) — clean; also `--features index` and
  `--features specta` — clean.
- `cargo clippy --all-targets -- -D warnings` (src-tauri) — clean; `--features sidecar-platform` — clean.
- `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` — regenerated;
  `bindings.gen.ts` diff is the propagated doc comment only, no signature change.
- `npm run check` — 0 errors, 0 warnings.

Docs: `docs/design/VAULT-SECURITY.md` §5 records the containment rule and §6 the review entry.

---

**2026-08-11 (review #1) — CHANGES REQUESTED closed on the same branch.**

**Finding (blocking, demonstrated — not probabilistic).** The first fix checked containment at **unlock
time only**. `VaultRegistry::unlock` stored the caller's raw, unresolved `session_dir` in the map, and
`lock_with_wiper` later read that stored path and called `wipe(&dir)` with **no re-check**;
`wipe_session_dir` used `Path::exists()` (follows reparse points) then `shred_tree` → `collect_files` →
`read_dir(root)` (also follows). `collect_files` skips symlinks found *inside* the tree, but nothing
rejected the **root itself** being a link. So what the guard actually contained was the caller's path
*string* at unlock time, not the *directory* that eventually got shredded — and those are separated in
time by a window the attacker fully controls (nothing wipes until `vault_lock` is called).

Reachable with three registered Tauri commands, no elevation:

1. `vaultUnlock(blob, pass, "<appCacheDir>/vault-sessions/<uuid>")` — passes containment legitimately.
2. `deletePermanent(["<appCacheDir>/vault-sessions/<uuid>"])` (or `moveExact`) — removes the real session dir.
3. `createJunction("C:\Users\me\Documents", "<appCacheDir>/vault-sessions/<uuid>")` — a Windows
   **junction** needs neither Developer Mode nor elevation (`crates/server/src/links.rs`).
4. `vaultLock(blob)` — every file under the victim directory is securely shredded.

**Fix — two independent guards, both fail closed.**

- **Re-validate at wipe time (`crates/server/src/vault_manager.rs`).** The registry now stores a `Session
  { dir, root }` instead of a bare `PathBuf`, so `lock_with_wiper` re-runs the *same* containment check
  (`ensure_contained(Guarded::Lock, &root, &dir)`) against the path **as it resolves now**, immediately
  before `wipe(&dir)`. `resolve_for_containment` canonicalizes the planted junction to the victim
  directory, which then fails `starts_with(root)`.
- **Belt-and-braces at the wipe itself.** `wipe_session_dir` now calls `std::fs::symlink_metadata` (which
  does *not* follow the link) and refuses outright when the session path `is_symlink()` — on Windows that
  covers junctions too. A genuine session dir is a real directory this module extracted into and is never
  a link, so there are no false positives. A missing path is still a no-op success, as before.

**Decide-and-log (what `lock` DOES when re-validation fails).** It **wipes nothing, drops the mapping, and
returns the refusal as an `Err`**. Rationale:

- *Not wiping* is the whole point — the shredder must never follow the swapped-in link.
- *Dropping the mapping* rather than keeping it retryable (which is what a failed **wipe** does) is the
  right asymmetry here. A failed wipe is transient — a locked file, a permissions blip — so staying
  "unlocked" keeps the lock retryable and honest about plaintext still being on disk. A failed
  *containment re-check* is not transient: every retry re-resolves the same tampered path and fails
  identically, so keeping the mapping would wedge the vault permanently "unlocked" with no user-reachable
  way to clear it. There is also nothing of ours left at that path to protect: removing the real session
  dir is a *precondition* of planting the link, so the plaintext we would have wiped is already gone.
  Dropping is therefore both safe and the only recoverable option — the user can simply unlock again.
- *Returning `Err`* (rather than a quiet `Ok`) so the tamper surfaces in the UI instead of being reported
  as a successful lock. The message is the house-style "refusing to lock: session directory … does not
  resolve inside the app's own vault-sessions directory …".
- The mapping is removed with the **same conditional check** the success path uses (`map.get(blob).dir ==
  dir`), so a concurrent re-unlock into a fresh, valid dir is never silently discarded.

**Review nits, also fixed.**

- *The guard mutated the filesystem.* `ensure_session_dir_contained` called `create_dir_all(sessions_root)`
  before checking, so a **refused** unlock still created `vault-sessions`. It is now **pure** — it only
  reads. A not-yet-existing root resolves through the same `resolve_for_containment` path a
  not-yet-existing session dir already used, and the root is created as a side effect of extracting into
  it (`write_tree_atomic` creates its staging sibling) *after* the check passes. The first-ever-unlock
  negative control still passes, and still asserts the root survives the lock.
- *Two adjacent same-typed `&Path` args.* Reordering alone would have left `sessions_root`/`blob_path`
  adjacent, so the root is **newtyped**: `SessionsRoot<'a>(&'a Path)`, and placed **first** in
  `unlock_to_session`, `VaultRegistry::unlock` and `unlock_with_wiper`. A transposed pair is now a type
  error, not a silently inverted guard. Sole external call site (`src-tauri/src/lib.rs`, `vault_unlock`)
  updated.

**Red-then-green probe (the reviewer's scenario, rebuilt from real filesystem objects).** Three new tests
in `crates/server/src/vault_manager.rs`:

- `lock_refuses_to_shred_a_victim_dir_junctioned_over_the_session_path` (Windows) — the exact exploit:
  legitimate unlock → `remove_dir_all` the session dir → plant a **junction** at that path pointing at a
  victim seeded with real files → `lock`. Hard-asserts the junction was created (NTFS needs no elevation),
  so this case cannot quietly not-run.
- `lock_refuses_to_shred_a_victim_dir_symlinked_over_the_session_path` — the symlink form, for the Linux/
  macOS legs of the 3-OS matrix; **skips loudly** (`eprintln!("SKIPPED …")`) if the OS won't make one.
- `wipe_session_dir_refuses_a_session_path_that_is_itself_a_link` — the belt-and-braces guard on its own,
  bypassing the registry entirely.

All three read the victim's bytes back **off disk**, never inferring safety from the returned `Err`. Their
scratch objects (including the junctions) are created inside `crates/server/target/cpe-1647-scratch/`, i.e.
inside the repo working tree, not the system temp dir.

With **both** new guards temporarily neutralized (lock-time re-check short-circuited, `wipe_session_dir`
reverted to its `exists()` form), all three **FAILED**, each with:

```
after locking a vault whose session dir was swapped for a junction: pre-existing file keepsake.txt is
gone/unreadable (The system cannot find the file specified. (os error 2)) — it was DESTROYED
...
test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 1929 filtered out
```

Restored → **3 passed; 0 failed**. (`assert_precious_intact` was switched from `.unwrap()` to an
`unwrap_or_else` panic so a regression reads as "it was DESTROYED" rather than an opaque `NotFound`.)

Verification (all synchronous, Windows):
- `cargo test` (crates/server) — **2070 passed, 0 failed**, 2 pre-existing ignored (was 2067; +3 new).
- `cargo clippy --all-targets -- -D warnings` (crates/server) — clean; `--features index` — clean.
- `cargo clippy --all-targets -- -D warnings` (src-tauri) — clean; `--features sidecar-platform` — clean.
- `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` — regenerated;
  `bindings.gen.ts` diff is the two propagated doc comments only, no signature change.
- `npm run check` — 0 errors, 0 warnings.

Docs corrected: `docs/design/VAULT-SECURITY.md` §5's containment bullet no longer overstates the
guarantee — it now spells out the three separate properties (validated at unlock, **re-validated at lock
immediately before the wipe**, plus the symlink/junction-root refusal) and says explicitly that the
original fix contained the path string, not the directory at wipe time. §6 gains a "review #1" entry
recording the demonstrated exploit and how it was closed.

**Deliberately out of scope:** `lock`'s wipe-failure retry semantics (unchanged), CPE-1645/CPE-1646, and
any new confirm flag. `sweep_orphan_sessions` was re-checked again and needs no change: it filters children
on `file_type().is_dir()`, which is `false` for a junction/symlink, so a planted link is skipped, not
followed.
