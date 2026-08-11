---
id: CPE-1630
title: "Vault create's \"securely delete the original\" has no confirmed flag — the second door into irreversible shredding, still open after CPE-1611"
type: Task
status: Doing
priority: Medium
component: Backend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent Reviewer of CPE-1611 (PR #819), and independently by that PR's UAT tester. CPE-1611
gave `shred_paths` an explicit `confirmed: bool` so the engine refuses unless told — closing the same class
of gap CPE-1599 closed for Batch Media. But `shred_paths` turns out not to be the only door.

## The gap
`vault_manager::shred_tree` (`crates/server/src/vault_manager.rs:466-479`) calls
`secure_shred::shred_file` **directly**, bypassing `shred_paths` and therefore the new gate entirely.

Upstream, `vault_create` (`src-tauri/src/lib.rs:6912-6928`, `crates/server/src/vault_manager.rs:134-170`)
takes `shred_original: bool` straight from `VaultCreateDialog.svelte`'s checkbox
(`src/lib/components/VaultCreateDialog.svelte:40,75,208`) with **no separate confirm flag layered on top of
that intent bool**. A devtools or automation caller can invoke `vault_create(folder, dest, pass, true)` and
skip the dialog's warning panel completely.

This is architecturally identical to the pre-CPE-1599 batch-media gap and the pre-CPE-1611 `shred_paths`
gap: `shred_original`'s danger is **conditional** (only when true), which is exactly the shape CPE-1599
established needs a distinct `confirmed` parameter rather than one intent flag doing double duty.

## What already mitigates it (so scope this proportionately)
This is **not** the same severity as an unguarded `shred_paths`. `vault_create` already:
- defaults `shred_original` to `false`,
- refuses if the destination blob resolves **inside** the folder being shredded, and
- performs a full decrypt round-trip verification of the on-disk encrypted copy **before** shredding the
  plaintext original — with dedicated tests (`shred_original_refuses_to_shred_when_verify_fails`,
  `shred_original_refuses_when_dest_is_inside_the_folder_to_be_shredded`).

So data is not *lost* — a recoverable copy is proven to exist first. But the plaintext original is still
irreversibly destroyed with no trash fallback, which is the same blast-radius class as `shred_paths`.

## Goal
One consistent rule across every destructive engine entry point: **the engine refuses unless explicitly
told, separately from the caller's intent flag.**

## Scope
- Add a `confirmed: bool` to `vault_create` (and thread it into `vault_manager::create_vault`), required
  only when `shred_original` is true; refuse cleanly with a specific error otherwise — never a panic, never
  a partial shred, and never a vault created but a shred silently skipped without saying so.
- `VaultCreateDialog.svelte` is the one and only caller allowed to pass `true`.
- Match CPE-1599's and CPE-1611's error shape and naming so all three refusals read alike.
- Regenerate `bindings.gen.ts` (the command signature changes) or the typed-bindings drift guard fails.
- While here, **audit for any third door**: grep for every direct `secure_shred::` / `shred_file` /
  `shred_tree` caller and list them in the work log, so the next person doesn't have to rediscover this.

## Acceptance criteria
- `vault_create(..., shred_original: true, confirmed: false)` shreds nothing and returns a specific error;
  a test verifies the originals' **bytes are intact on disk**, not merely that an `Err` came back.
- The confirmed path still works end-to-end, including the existing verify-before-shred guarantee.
- The audit list of all direct shred callers appears in the work log.
- Tests must not assert exact filesystem byte counts (CI runs Linux + macOS + Windows).

**Conflict surface:** `crates/server/src/vault_manager.rs`, `src-tauri/src/lib.rs` (the `vault_create`
command + `generate_handler!`), `src/lib/bindings.gen.ts`, `src/lib/components/VaultCreateDialog.svelte`,
plus tests. Touches `lib.rs` and `bindings.gen.ts` — do not run in parallel with other command-signature work.

## Work Log

**2026-08-11 — implemented, tested, PR opened (sprint worker).**

Matched CPE-1599's / CPE-1611's shape exactly:

- `crates/server/src/vault_manager.rs`: `create_vault` (and the injectable `create_vault_with_verifier`)
  now take a `confirmed: bool` parameter, separate from `opts.shred_original`. When `shred_original` is
  `true` and `confirmed` is `false`, `create_vault` returns `Err(VaultError::Format(...))` **before
  sealing even starts** — no blob is written, nothing is encrypted, the plaintext original is completely
  untouched. The refusal message follows `shred_paths`' wording pattern (contains "confirm", names the
  one authorized caller). When `shred_original` is `false`, `confirmed` is ignored (sealing without
  shredding always succeeds regardless of its value) — matches the ticket's "required only when
  `shred_original` is true". The pre-existing checks (dest-inside-folder guard, then encrypt, then
  verify-before-shred, then the actual `shred_tree`) are unchanged and still run in the same order,
  strictly after the new confirm gate.
- `src-tauri/src/lib.rs`: the `vault_create` Tauri command gained a `confirmed: bool` parameter and
  threads it straight into `cpe_server::vault_manager::create_vault`. Still a thin one-line dispatcher.
  Confirmed registered by name in **both** `generate_handler!` call sites (line ~11466 in `run()` and
  line ~12283 in `export_bindings`) — neither needed edits since they list commands by bare identifier,
  not by signature.
- `src/lib/components/VaultCreateDialog.svelte`: `create()` now calls
  `commands.vaultCreate(folderPath, dest, passphrase, shredOriginal, shredOriginal)` — this dialog is the
  ONE place in the codebase allowed to pass `confirmed: true`, and its own submit (checkbox + inline
  warning already rendered above the button) IS the confirmation, so `confirmed` tracks `shredOriginal`
  rather than being a separate UI control. Doc comment updated to explain the CPE-1630 gate.
- `src/lib/bindings.gen.ts`: regenerated via `cargo run --bin export_bindings --features
  "specta-bindings sidecar-platform"` from `src-tauri/` — not hand-edited. Only the `vaultCreate` entry
  changed (new `confirmed` param + doc comment); diff is 7 insertions / 2 deletions.
- Tests added in `crates/server/src/vault_manager.rs`:
  - `create_vault_refuses_the_whole_call_when_shred_original_is_not_confirmed` — `shred_original: true,
    confirmed: false` → `Err` (message contains "confirm"), then reads the original files' **bytes back
    off disk** (`top.txt` == `b"top secret"`, `sub/inner.bin` == the exact fixture bytes) rather than
    trusting the `Err`, and asserts no vault blob was written at all.
  - `create_vault_proceeds_and_still_verifies_before_shred_once_confirmed_is_true` — the identical call
    with `confirmed: true` succeeds, the original is gone, and the sealed blob still round-trips through
    a real unlock (proves CPE-1630 doesn't regress CPE-1248's verify-before-shred invariant).
  - `confirmed_is_ignored_when_shred_original_is_off` — `shred_original: false, confirmed: false`
    succeeds normally and never touches the original.
  - The 14 pre-existing `create_vault`/`create_vault_with_verifier` call sites in this file's test module
    were all updated to pass the new `confirmed` argument (`false` where `shred_original` is off/default,
    `true` where an existing test is specifically exercising the dest-inside-folder guard, the
    unextractable-name refusal, or the verify-before-shred invariant — so those tests keep testing what
    they were named for, not the new confirm gate).
- `src/lib/components/VaultCreateDialog.test.ts`: the existing invoke-args assertion for `vault_create`
  now expects `confirmed: false` (shred unchecked); added a new test
  (`passes confirmed: true alongside shredOriginal: true when the shred checkbox is checked`) asserting
  the dialog wires `confirmed` to `shredOriginal`, not a hardcoded value.
- No changes needed to `src/App.paneBArchiveVault.test.ts` — its mock reads only `folder`/`dest`/
  `shredOriginal` off the invoke args by name (doesn't do a full-object equality against the raw IPC
  payload), so it's unaffected by the new `confirmed` argument; ran it anyway to confirm (see below).
- No locale/`$t()` changes: like `ShredConfirmDialog.svelte`'s CPE-1611 precedent, `VaultCreateDialog.svelte`
  surfaces the raw backend error string directly (`error = String(e)`) rather than through `$t()` — neither
  dialog calls `$t()` anywhere, so there was nothing to add to the 12 locale catalogs.

**Negative control (run before committing the fix):** temporarily neutered the new guard
(`if !confirmed && false /* ... */`) and re-ran
`cargo test create_vault_refuses_the_whole_call_when_shred_original_is_not_confirmed` — it FAILED as
expected:
```
thread '...create_vault_refuses_the_whole_call_when_shred_original_is_not_confirmed' panicked at
src\vault_manager.rs:729:14:
an unconfirmed shred_original create_vault call must be refused, not executed: ()
```
i.e. without the gate, the call silently succeeded and shredded the original — proving the test isn't
vacuously true. Restored the guard immediately after and re-ran green.

**Third-door audit (the ticket's ask) — every direct `secure_shred::` / `shred_file` / `shred_tree` /
`shred_paths` caller in the tree** (excluded sibling agent worktrees under `.claude/worktrees/*`, which
are other in-flight sessions' copies of the same files, not separate call sites):

| Caller | File | Gated? |
|---|---|---|
| `secure_shred::shred_paths` (public API, dispatched by the `shred_paths` Tauri command) | `crates/server/src/secure_shred.rs` / `src-tauri/src/lib.rs:2138` | Yes — `confirmed: bool` since CPE-1611 |
| `vault_manager::shred_tree` → `secure_shred::shred_file` (the ONE place `shred_file` is called directly, bypassing `shred_paths`) | `crates/server/src/vault_manager.rs:474` (line numbers pre-CPE-1630; now further down after the new gate) | **This ticket** — gated indirectly: `shred_tree`'s only two callers are `create_vault` (now gated by the new `confirmed` check, added above) and `wipe_session_dir` (see next row) |
| `vault_manager::wipe_session_dir` → `shred_tree` | `crates/server/src/vault_manager.rs` (called from `VaultRegistry::lock`/`unlock_with_wiper`'s stale-session cleanup and `sweep_orphan_sessions`) | **Not gated, and intentionally not in scope.** This wipes an already-*extracted session directory* (a transient decrypt artifact the app itself created when unlocking a vault) — not a user's original file — as part of the ordinary "lock a vault" / "clean up an orphaned session on startup" flow. There is no user intent to confirm here: locking IS supposed to wipe the plaintext session copy, every time, by design (see the module's own doc comment on the "mount tradeoff"). Flagging this for awareness, not as a follow-up ticket — it's a different risk class (transient app-owned scratch data, not a user's file) from `shred_paths`/`create_vault`'s original-file destruction. |

No other crate (`crates/{contract,ftp,mdns,net,security,sftp,updater-verify,vfs,webdav}`,
`sidecar/{agent-board,ai-console,contract,host,repos}`) references `secure_shred`, `shred_file`,
`shred_tree`, or `shred_paths` at all — `cpe-server` and `src-tauri` are the only two crates that touch
this module. So as of this ticket, every conditionally-destructive shred entry point that a caller
outside its own confirm dialog could reach (`shred_paths`, `vault_create`) now refuses without an
explicit `confirmed: true`.

**Verification (all run synchronously in the worktree, from
`Z:\repos\cross-platform-explorer\.claude\worktrees\agent-ab16d802bd6343fee`):**
- `cargo build` (crates/server) — pass.
- `cargo test vault_manager` (crates/server) — 23 passed, 0 failed (20 pre-existing + 3 new).
- `cargo test` (crates/server, full suite) — 1906 passed, 0 failed, 2 ignored.
- `cargo clippy --all-targets -- -D warnings` (crates/server, default features) — clean.
- `cargo clippy --all-targets --features specta -- -D warnings` (crates/server) — clean.
- `cargo build` (src-tauri, default features) — pass.
- `cargo test` (src-tauri) — 144 passed, 0 failed.
- `cargo clippy --all-targets -- -D warnings` (src-tauri, default features) — clean.
- `cargo clippy --all-targets --features sidecar-platform -- -D warnings` (src-tauri) — clean.
- `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` (src-tauri) — pass,
  regenerated `bindings.gen.ts` (minimal diff: `vaultCreate` gains `confirmed`).
- `npm run check` — 0 errors, 0 warnings.
- `npx vitest run` (full suite) — 283 files / 3490 passed, 0 failed (baseline before this ticket's new
  tests: 3488 passed, 1 failing on the stale invoke-args assertion this ticket fixes).

No new dependencies added; no `Cargo.lock` changes. Diff kept to the conflict surface the ticket named
plus the two test files (`vault_manager.rs`'s inline `#[cfg(test)] mod tests` and
`VaultCreateDialog.test.ts`) that needed updating for the new parameter.
