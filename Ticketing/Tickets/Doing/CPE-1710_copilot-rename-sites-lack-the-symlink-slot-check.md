---
id: CPE-1710
title: copilot's rename and transfer sites destroy a dangling symlink at the destination
type: bug
priority: Medium
status: Doing
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Found by the PR #893 (CPE-1705) reviewer, 2026-08-13, while enumerating that ticket's sites rather than
spot-checking them.

`copilot::apply_op` (the `Rename` arm) and `copilot::transfer_entry` are both **`fs::rename`-destructive**
at the destination. Both received the `clobber_refusal` guard in CPE-1705 — but **neither got
`symlink_slot_refusal`**, which `rename_entry_impl` and `move_exact_impl` both have.

The consequence: **a dangling symlink sitting at the destination is silently destroyed.** `clobber_refusal`
answers "is something already here?" using a stat that follows the link; for a link whose target does not
exist, that answers *no*, the slot reads as free, and the rename replaces the link itself.

The PR's own helper doc comment states that a `rename`-destructive site needs the extra symlink check.
These two sites are the exceptions to a rule the same PR wrote down.

## Why it is Medium and not High

A dangling symlink is a less common thing to lose than a file with contents, and the loss is of the link
rather than of data — the link's target was already absent. It is still a silent destruction of something
the user created, at a site whose two siblings guard against exactly this.

## Scope

`copilot::apply_op`'s `Rename` arm and `copilot::transfer_entry`. Compare against `rename_entry_impl` and
`move_exact_impl`, which are the correct shape.

## Acceptance criteria

- [ ] Both sites apply `symlink_slot_refusal` alongside `clobber_refusal`, matching `rename_entry_impl`.
- [ ] A test proves a **dangling** symlink at the destination survives, for each of the two sites, and that
      removing the check turns a **distinct** test red. Assert on the slot still being a symlink after the
      call — not on the returned `Result`, which was `ok: true` in the reviewer's reproduction.
- [ ] Check whether any **other** `fs::rename`-destructive site is missing the pairing. The reviewer found
      these two by enumeration; enumerate again rather than fixing only the two reported. If the pairing is
      always required, consider making it structurally impossible to apply one without the other rather
      than relying on every future author remembering.
- [ ] Platform-gate correctly. Symlink creation on Windows needs either Developer Mode or elevation, so a
      test that silently no-ops on an unprivileged runner proves nothing — detect and skip **loudly** with a
      `writeln!(stderr)` notice, and make sure the Linux and macOS legs assert something real. CI runs a
      3-OS matrix.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #893 review, 2026-08-13, on the reviewer's recommendation to handle it as
a follow-up rather than widening that PR.

**Useful technique, measured on this sprint:** a slot whose stat is genuinely refused can be staged locally
on Windows two independent ways — deny `(R)` on the target **plus `RD` on its parent** (which kills
`fs::metadata`'s `FindFirstFileW` fallback), or a **symlink whose resolution target is denied**. The second
exercises the reparse path and is the more natural fit here. See CPE-1705's "CORRECTION 4" section; that
ticket's guidance was wrong four times before this was understood, so read it before writing an ACL test.

Related: **CPE-1705** (which added `clobber_refusal` to these sites), **CPE-1687** (the honest-refusal
wording pattern), **CPE-1696** (the sibling stat-collapse round).

## Work Log

**2026-08-13 — worked to a pushed PR (branch `cpe-1710-copilot-symlink-slot`).**

### What the fix is

`fsutil::rename_slot_refusal(target, occupied)` — `clobber_refusal` then `symlink_slot_refusal`, in the
order the two correct sites already used, as **one call that cannot be half-applied**. The reported two
sites were fixed by converting to it, and so were the other two the enumeration turned up.

### Enumeration — every `fs::rename` in the tree, and its guard status

Scope of the search (stated, per Evidence Rule 2): `grep -rn "fs::rename"` over `crates/`, `src-tauri/src/`
and `sidecar/`, then each hit read in context.

**Rename-destructive at a user-named slot — the class this ticket is about (6 sites):**

| Site | Before | After |
|---|---|---|
| `copilot::apply_op` `Rename` arm (`copilot.rs:233`) | `clobber_refusal` only | `rename_slot_refusal` |
| `copilot::transfer_entry` (`copilot.rs:267`) | `clobber_refusal` only | `rename_slot_refusal` |
| `organize_apply::apply_proposals` (`organize_apply.rs:99`) | **`clobber_refusal` only — same bug, not reported** | `rename_slot_refusal` |
| `ticket_move` board move (`src-tauri/src/lib.rs:167`) | **`clobber_refusal` only — same bug, not reported** | `rename_slot_refusal` |
| `rename_entry_impl` (`src-tauri/src/lib.rs:1842`) | both, open-coded | `rename_slot_refusal` |
| `move_exact_impl` (`src-tauri/src/lib.rs:3408`) | both, open-coded | `rename_slot_refusal` |

So it was **four of six** missing the pairing, not two of six. That is the argument for making it
structural: CPE-1705 wrote the rule into a doc comment and two thirds of its own sites did not follow it.

### Round 1's out-of-class list was ASSUMED. Round 2 traced it — and it was wrong twice

The PR #895 UAT rejected the first version of the list below, correctly. It said "the destination is a file
we own" of twelve sites without checking, and the reason it was wrong is worth writing down: the framing
was *"only a dangling link is at risk, because `clobber_refusal` catches the live ones"* — which holds
**only where `clobber_refusal` is actually called**. Carrying that assumption from the guarded sites onto
an unguarded one is what hid a live user-data bug.

Every entry below is now traced back to where its destination path comes from.

**Genuinely app-private (destination cannot be steered onto a user-named file):**

| Site | Destination provenance |
|---|---|
| `audit_journal.rs:119` | `app_data_dir()/audit`, leaf = session id **sanitised** to `[A-Za-z0-9_-]` |
| `checkpoint_store.rs:294` | `app_data_dir()/checkpoints/<hex digest of root>` — the root is hashed, never joined |
| `metrics_journal.rs:124` | `app_data_dir()/agent-metrics/history.jsonl`, fixed leaf |
| `replay_baseline.rs:133` | `app_data_dir()/audit`, leaf = sanitised session id + fixed suffix |
| `known_hosts.rs:189` | `default_app_known_hosts_path()` — the app's own config dir, never `~/.ssh` |
| `index.rs:615` | `app_data_dir()/index/{volume_id}.idx`; `volume_id` is a `u64`, so it cannot carry a separator |
| `semantic_index.rs:225` | `app_data_dir()/content-index/{fnv1a64(root):016x}.cix` — root hashed |
| `vector_index.rs:223` | **no production caller at all** (only its own test); prod persists via `SemanticIndex::save` |

**User-reachable destinations — three of them, and the first version called all three "a file we own":**

- **`src-tauri/src/lib.rs:3546` (`metadata_write`)** — `path` straight off IPC; the final rename lands on
  the **user's own media file**, and it has **no guard of any kind**, so a **live** symlink is destroyed
  and the edit is lost. Found by the UAT, **filed as CPE-1716 (High)**, not fixed here.
- **`vault_manager.rs:248` (`create_vault`)** — `dest` is a user-chosen `.cpevault` path from the create
  dialog. Only the *staging* name is app-owned; the final rename replaces `dest` including a link at it.
  Guarded by `create_staging_exclusive` (`O_EXCL`) and a `confirmed` gate, with no occupancy/symlink check
  on the final rename.
- **`vault_manager.rs:745` (`reseal_session`)** — `blob_path` is the user's vault file. Heavily guarded
  (`symlink_metadata` refusal on the session dir, containment checks, `ensure_no_aliased_files` twice,
  `O_EXCL` staging), and the final replace-in-place is the **documented CPE-1670 decision**.

  Flagged rather than changed: the vault's replace-in-place is a deliberate recorded design decision, and
  reopening it belongs with someone holding that ticket's context, not inside this one.

- `vault_crypto.rs:542` (`promote`) — user-supplied `session_dir`, but `ensure_session_dir_contained`
  requires it to resolve inside the app's own `vault-sessions` root, and `promote` refuses a non-empty
  existing `out_dir`.

**Still out of class, with the checked reason:**

- **`provider.rs:156` (`LocalProvider::rename`)** — an unguarded `std::fs::rename` on raw path arguments
  that appeared in **neither** of round 1's lists (the UAT caught the omission). Traced: it has **no
  production caller** — the only construction of `LocalProvider` is `fs_route::provider_for`, called
  solely from that file's own tests; `cpe_vfs::connect` exposes no rename. Real renames go through
  `rename_entry_impl`. So: not user-reachable **today**, and it would need the pairing the moment it is
  wired up.
- **Name-picking probes rather than refusals** — `unique_target` → `do_move_into`, and `resolve_conflict`.
  These *advance past* an occupied slot, so `rename_slot_refusal` is the wrong shape. **Real residual
  instance**, filed as **CPE-1715**.
- **Protocol server rigs** — `crates/ftp`, `crates/sftp`, `crates/webdav` implement a wire protocol's own
  rename semantics against a sandbox root.
- **`clobber_refusal` sites that are not renames** — `folder_template` and `src-tauri` trash-restore (×2)
  precede `fs::write` or an OS restore. **`split_join` was wrongly included here**: the UAT showed
  `join_files` follows a link at `out_path` — deleting it on the failure path, and writing the user's
  bytes *through* it on the success path. **Filed as CPE-1718.**
- **No other `fs::rename` exists** in `src-tauri/src/` outside `lib.rs` (nine sibling files, zero hits) or
  under `sidecar/` in non-test code (the two hits are inside `catalog.rs`'s `#[cfg(test)]` module).

### Round 3 — the structural guard is clippy, not a source scan

The reviewer (CHANGES REQUESTED) reproduced the >25-line and alias bypasses independently before seeing
the UAT's report, and recommended the shape that actually works. Adopted:

- **`clippy.toml` with `disallowed-methods = [{ path = "std::fs::rename", … }]`** in each workspace root
  that renames — `crates/server`, `src-tauri`, `crates/ftp`, `crates/sftp`, `crates/webdav`,
  `sidecar/host`. (There is no root `Cargo.toml`; every one of these is an independent root.)
- **`fsutil::rename_into_slot(src, target, occupied)`** does the pairing *and* the rename, carrying the
  single `#[allow]` for the guarded path. All six user-named-slot sites call it.
- **Every other `fs::rename` carries `#[allow(clippy::disallowed_methods)]` with a one-line reason at the
  site** — 17 of them, each naming why that destination is not a user-named slot.

Why this and not a tighter scan:

- It catches the **unguarded** rename, which the scan is structurally blind to and which is the more
  dangerous case.
- It resolves **paths, not text**, so all three demonstrated bypasses stop working.
- It rides a gate that already exists: CI runs `clippy --all-targets -- -D warnings` in both feature modes
  for both workspaces, and `ci.yml` already enforces that every `sidecar/*` directory is clippy'd. The
  source scan was never going to reach `sidecar/`; this does, for free.
- The decisive one: **the out-of-class justification now lives in the code, at the site.** Round 1's lived
  in a PR description, and it was wrong twice.

**Known gap, stated rather than solved:** `disallowed_methods` cannot see a rename reached through a `dyn`
trait object, so `Provider::rename` is not covered by it. That is written into the `clippy.toml` comment
and at `provider.rs` itself.

### Round 3 — a second wrong out-of-class entry, fixed here

**`vault_crypto::promote`.** `out_dir` is the user's unlock destination, not a file this crate owns. Its
emptiness probe is `read_dir`, which follows the link. Both legs are now guarded and tested, and the
measurement is narrower than the report in one direction and worse in the other:

- **Dangling link:** the pre-fix code does **not** destroy it — renaming a *directory* onto a
  non-directory entry is refused by the OS first (`ENOTDIR`; "The directory name is invalid", os error
  267, on Windows). So that leg's pre-fix bug is a confusing OS error, not data loss. Recorded honestly in
  the test rather than claimed as destruction.
- **Live directory link over an empty target:** `read_dir` follows it, finds the target empty, and
  `remove_dir(out_dir)` deletes **the link**; the rename then succeeds. Measured: `result was Ok(())` with
  the link replaced by a real directory. That is the real loss, and it is the `metadata_write` shape.

`vault_manager` ×2 reached the right conclusion for the wrong stated reason — the destination is the
user's chosen `.cpevault` path and replacing it is a documented CPE-1670 decision. The justification is
fixed at both sites; the code is not.

### The scan is a lint for one shape — NOT a structural guarantee

Round 1 claimed this made the pairing structurally impossible to get wrong. The UAT bypassed it three
ways, each measured, and the first bypass destroyed a link with the scan green:

| Smuggle | Scan |
|---|---|
| guard and `fs::rename` 30 lines apart | green |
| `use std::fs::rename as move_entry;`, guard and rename adjacent | green |
| rename behind a 3-line helper | green |

Worse, it only catches sites carrying **half** the guard: an `fs::rename` with **no** guard, or the
pre-CPE-1705 `if dst.exists()` shape, is invisible to it. The claim is withdrawn. What it is now:

- Renamed `half_applied_rename_guards_are_rejected`, with the three bypasses and the unguarded-rename hole
  written into its doc comment and restated in its own failure message, so nobody reads it as a class
  guarantee again.
- **Stops crying wolf.** It respects function boundaries (the UAT tripped the first version on a guard 8
  lines above a rename *in a different function*) and skips test modules (a unit test that asserts
  `clobber_refusal` while using `fs::rename` to stage a scenario is not a half-guarded site). Both
  behaviours are unit-tested in `the_scan_window_stops_at_a_function_boundary`, including an assertion
  that the beyond-the-window hole is still there — so the doc comment cannot quietly stop being true.
- **Widened** from "`crates/server/src` + `src-tauri/src/lib.rs`" to every `.rs` under `crates/*/src`,
  `src-tauri/src` and `sidecar/*/src`. The old scope missed nine files in `src-tauri/src/` alone, and
  skipped any file named `fsutil.rs` **anywhere** by basename; the exemption is now the full path of this
  one module.

A guard that genuinely closed the class would have to find every `fs::rename` whose destination is
user-named and require the pairing there. That is a different and much larger piece of work; it is not in
this PR and is not claimed to be.

### Evidence (Evidence Rules, `Ticketing/wiki.md`)

Committed **before** probing. Each guard broken **on its own**, restored with `git checkout --`, real
recompiles observed. Full output pasted in the PR body. **All six sites now have a test**, and each break
reds a **distinct** one:

1. `copilot` `Rename` arm → only `cpe_1710_execute_never_renames_over_a_dangling_link_at_the_new_name`
   (+ the scan, by design — it is the shape that is wrong). The other site tests stayed green.
2. `copilot::transfer_entry` → only `cpe_1710_execute_never_moves_over_a_dangling_link_at_the_destination`.
3. `organize_apply` → only `cpe_1710_organize_never_renames_over_a_dangling_link_in_the_destination_folder`.
4. `rename_entry_impl` → only `cpe_1710_rename_entry_never_renames_over_a_dangling_link_at_the_new_name`.
5. `move_exact_impl` → only `cpe_1710_move_exact_never_renames_over_a_dangling_link_at_the_destination`.
6. `board_move_impl` → only `cpe_1710_board_move_never_renames_over_a_dangling_link_at_the_destination`.
7. `rename_entry_impl` re-separated into two calls → the scan fires **both** of its rules at once.

Rounds 1's last three sites (`rename_entry_impl`, `move_exact_impl`, `board_move_impl`) had **no test
each** and leaned entirely on the scan, which the UAT then showed is bypassable — a lint is not a test.
The blocker was real: `make_dangling_link` was `#[cfg(test)] pub(crate)` in `cpe-server` and unreachable
from the app adapter. It is now `pub`, so there is one implementation instead of a third inlined copy.

Each test asserts on the **slot** (`symlink_metadata(...).is_symlink()`) **before** touching the returned
`Result`, deliberately: an `expect_err` first would red on the result, and the whole bug is that the result
looked fine. The red output shows it — *"the user's link was DESTROYED by the rename (result was
Ok(\"…\\b.txt\"))"*.

### Platform gating

No ACLs are needed here at all: a *dangling* link is an ordinary object, and `try_exists` answers
`Ok(false)` for one on every platform. Only **creating** the link can be refused. `fsutil::make_dangling_link`
tries `symlink_file` first (needs Developer Mode / elevation on Windows) and falls back to an NTFS
**junction** (no privilege — created against a real directory that is then removed), so the Windows leg
asserts for real on an unprivileged runner too. If both fail it is a `writeln!(stderr)` skip that says
nothing was covered — **but see the caveat below: that notice is not visible under CI's `cargo test`.**

**The skip notice is invisible in CI, and this PR does not claim otherwise.** `.github/workflows/ci.yml`
runs `cargo test` with no `--nocapture`, and libtest captures stderr for *passing* tests — and a skip is
a pass. So on a Windows runner that could create neither a symlink nor a junction, these tests would pass,
print nothing anyone sees, and cover nothing. That is true of CPE-1705's notices as well. Filed by the
Foreman as **CPE-1717 (High)**; not fixed here.

**Round 1's evidence for this was true but did not prove itself**, as the UAT pointed out: "verified with
`--nocapture`, no skip notice" only establishes that *a* link was created, and this machine has Developer
Mode on, so leg 1 always won and **the junction fallback CI depends on never ran**. Round 2 drives leg 2
directly instead of inferring it — `the_junction_fallback_stages_the_same_hazard_as_a_symlink` builds a
junction, deletes its target, and asserts the resulting slot is the same hazard: a link by
`symlink_metadata`, invisible to `clobber_refusal`, refused by `rename_slot_refusal`. That runs on every
Windows CI leg regardless of the runner's privilege state.

### Checks

`cargo test` and `cargo clippy --all-targets -- -D warnings` clean in `crates/server` for **both** CI
feature modes (default and `--features index`); `src-tauri` clippy clean in both modes (default and
`--features sidecar-platform`) and `cargo test` green.

### Follow-ups filed

- **CPE-1715** — `unique_target` / `resolve_conflict` treat a dangling link as a free name, so a bulk move
  auto-renames *onto* it. Different fix shape (treat as occupied, pick the next name), hence its own
  ticket.
- **CPE-1718** — `join_files` follows a link at `out_path`: the failure path `remove_file`s the user's
  link, and the success path writes their bytes *through* it to a path they never named. Round 1 wrongly
  classified `split_join` as safe on the strength of "it precedes `File::create`" — the `File::create` is
  what makes it worse.
- **CPE-1716** (filed by the Foreman from the UAT, High) — `metadata_write` renames onto the user's own
  media path with **no guard at all**, destroying even a **live** symlink and losing the edit while
  reporting success.
