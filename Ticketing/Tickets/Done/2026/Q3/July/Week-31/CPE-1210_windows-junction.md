---
id: CPE-1210
title: "Backend (Windows, OS-gated): junction creation + New Link 'Junction' kind"
type: feature
component: Backend
priority: low
status: Done
tags: ready
created: 2026-08-01
epic: CPE-715
---

## Summary
Part of CPE-715. Windows directory junctions (the Windows dir-link). OS-gated — needs a reparse-point syscall
and the Windows CI runner for verification.

## Build
- `#[cfg(windows)] create_junction(target, link_path)` via reparse-point DeviceIoControl (or the `junction`
  crate — flag the dep for review). Tauri command + binding. Add "Junction" as a third kind in the New Link
  dialog (CPE-1207), shown only on Windows.

## Acceptance Criteria
- [x] cargo test (Windows-gated) creates + resolves a junction; skipped elsewhere. **Attended cross-OS verify
      flagged** (Windows CI runner). clippy clean; `npm run check` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-715). OS-gated; after CPE-1207. Build+unit-test what's
  possible; Windows-runner verifies.
- 2026-08-01 — Done. Built directly on this Windows worker host, so the junction path is locally testable
  (not just build-gated). `crates/server/src/links.rs` gained `create_junction(target, link_path)`:
  `#[cfg(windows)]` real implementation using the **`junction` crate** (`junction = "1"`, resolved to
  `1.4.2`), added Windows-only under `[target.'cfg(windows)'.dependencies]` in `crates/server/Cargo.toml`
  (same block `winreg` already lives in) — took the small, focused dep over hand-rolling the
  `DeviceIoControl` reparse-point buffer layout, per the ticket's stated preference; **flagged for review**.
  Validates the target is a directory first (junctions can't target a file) and returns a clear error
  otherwise, rather than a raw syscall error. A `#[cfg(not(windows))]` stub returns a clear
  "Directory junctions are Windows-only" error so the fn always exists (no `#[cfg]` needed at the Tauri
  command layer). Two new `#[cfg(windows)]` tests: `junction_resolves_to_target_directory` (creates a
  junction to a temp dir, reads a file through the junction path, and confirms `link_status` reports it
  as a reparse point / non-broken) and `junction_rejects_file_target`. Both run for real on this Windows
  host — unlike the existing symlink tests, junction creation needs no Developer Mode / elevation, so
  these aren't gated behind the unprivileged-Windows skip pattern.
  `src-tauri/src/lib.rs` got a thin `spawn_blocking` `create_junction` command (registered in both
  `generate_handler!` and the specta `collect_commands!` block) and `bindings.gen.ts` was regenerated
  (`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`) — added `createJunction`.
  `NewLinkDialog.svelte` gained "Junction" as a third `kind`, gated `{#if isWindows}` using the same
  `navigator.platform`/`navigator.userAgent` sniff `ShellIntegration.svelte` already uses for its
  Windows-only shell-integration row (no new detection plumbing invented). Picking Junction flips the
  native target picker to directory mode (junctions can only target a folder) and shows an inline hint
  under the target field; Create dispatches `commands.createJunction`. i18n: `link.kindJunction` +
  `link.junctionTargetHint` added across all 12 complete locales (`src/lib/i18n.ts`), verified by the
  existing `i18n.test.ts` locale-completeness guard.
  Verification, all synchronous on this Windows host: `cargo test -p cpe-server` (from
  `crates/server`) green, 19/19 `links::` tests incl. both new junction tests; full `cargo test` (both
  `crates/server` and `src-tauri`) green, including `src-tauri`'s `typed_bindings_are_committed_and_
  routed_through_busy_cursor` drift guard (bindings.gen.ts committed + current); `cargo clippy
  --all-targets -- -D warnings` clean on `cpe-server` (default, `--features specta`, `--features index`)
  and on `src-tauri` (default, `--features "specta-bindings sidecar-platform"`); `npm run check` 0
  errors; `npm test` 143 files / 1587 tests green, incl. 3 new `NewLinkDialog.test.ts` cases (Junction
  shown only when `navigator.platform` says Windows, hidden off Windows, and the create_junction call +
  directory hint). `gui-smoke` spec `new-link.smoke.ts` wasn't touched — it drives Hardlink only by
  deliberate original design (unprivileged-safe across all 3 CI OSes) and still typechecks against the
  unchanged dialog shape; a fresh Junction gui-smoke leg isn't addable in this pass since `gui-smoke/`
  has no installed `node_modules` in this worktree (pre-existing gap, unrelated to this change) so
  `npm run typecheck` there couldn't be run — noted honestly rather than skipped silently. Cross-OS
  (Linux/macOS) verification of the `#[cfg(not(windows))]` stub path is left to the 3-OS CI matrix, per
  the ticket's "Windows CI runner" flag — not independently re-verified here beyond `cfg`-gated
  compilation reasoning.
