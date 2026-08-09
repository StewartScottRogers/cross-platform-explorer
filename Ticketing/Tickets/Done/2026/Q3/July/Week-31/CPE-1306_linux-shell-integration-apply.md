---
id: CPE-1306
title: "Linux shell-integration apply/remove glue"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-712
---

## Summary
`crates/server/src/shell_menu.rs` has a fully-tested, pure `linux_shell_plan` (CPE-1021) — the `.desktop`
file path + contents to install under `~/.local/share/applications` — but no code actually writes it. The
`#[cfg(not(windows))]` stub for `install_shell_integration`/`uninstall_shell_integration`/
`shell_integration_installed` returns `Err("...only implemented on Windows so far")` for every non-Windows
OS, including Linux. This ticket builds the real Linux apply/remove glue, mirroring the already-shipped
Windows registry glue (CPE-1020).

## Build
- `#[cfg(target_os = "linux")]` apply: write the `.desktop` file from `linux_shell_plan` to its target path
  (creating parent dirs), then best-effort shell out to `xdg-mime`/`update-desktop-database` — their
  failure is non-fatal; the file write is the source of truth.
- `#[cfg(target_os = "linux")]` remove: delete the installed `.desktop` file, idempotent (absent = Ok).
- `shell_integration_installed` on Linux = the `.desktop` file exists.
- Keep the OS-agnostic path/content decision in the existing non-cfg-gated `linux_shell_plan` (already
  unit-tested on every host, including this Windows box); only the actual `fs::write`/`fs::remove`/shell-out
  is cfg-gated + thin, matching the Windows `apply_entries`/`remove_keys` pattern.
- The remaining `#[cfg(not(windows))]` stub (now covering macOS + other non-Windows/non-Linux OSes only)
  keeps returning the "not implemented" error so the contract still compiles everywhere.
- `src-tauri/src/lib.rs` command layer (`install_shell_integration`/`uninstall_shell_integration`/
  `shell_integration_installed`) is already OS-generic and registered — no changes there.

## Acceptance criteria
- `cargo test -p cpe-server` (from `crates/server`) green, including new OS-agnostic-reachable coverage.
- `cargo check --target x86_64-unknown-linux-gnu` (from `crates/server`) compiles clean — proof the Linux
  `#[cfg(target_os = "linux")]` arm type-checks, even though real Linux behaviour needs CI's Linux leg.
- `cargo clippy --all-targets -- -D warnings` clean on the Windows default target.
- Real end-to-end Linux behaviour (install → `.desktop` file appears + is valid → uninstall → no residue)
  is confirmed by CI's Linux leg after merge.

## Notes
Epic CPE-712 ("Shell citizen"), currently Proposed/dormant — its 2026-07-30 DoD review flagged "Linux/macOS
apply glue unbuilt" as remaining scope. This ticket closes the Linux half; macOS stays Mac-gated.

## Work Log
- 2026-08-03 — Filed and worked in the same pass (sprint). Built `apply_desktop_file`/
  `remove_desktop_files` + `linux_plan_for_current_user` in `crates/server/src/shell_menu.rs`, cfg-gated to
  `target_os = "linux"`, wrapping the existing pure/tested `linux_shell_plan`. `install_shell_integration`
  writes the `.desktop` file (creating parent dirs) then best-effort shells to `xdg-mime`/
  `update-desktop-database`; `uninstall_shell_integration` deletes it (idempotent); `shell_integration_installed`
  checks file existence. The macOS/other stub narrowed from `#[cfg(not(windows))]` to
  `#[cfg(not(any(windows, target_os = "linux")))]`. Added two Linux-only round-trip tests
  (`linux_apply_then_remove_roundtrips_and_is_idempotent`, `linux_shell_integration_installed_reflects_the_real_file`)
  against a scratch tempdir `$HOME`, mirroring the CPE-1020 Windows registry test. `src-tauri/src/lib.rs`
  command layer untouched (already OS-generic). Verified: `cargo test` 1463 green; `cargo clippy --all-targets
  -- -D warnings` clean (default + `index` + `pdf-thumb,video-thumb` feature modes); `cargo check --target
  x86_64-unknown-linux-gnu` **and** `cargo clippy --target x86_64-unknown-linux-gnu --all-targets -- -D
  warnings` both clean (cross-compiled locally via a `zig cc`-backed `CC_x86_64_unknown_linux_gnu` wrapper,
  worked around this box's missing native Linux cross-gcc — the baseline/unmodified tree hit the same
  missing-toolchain error before this workaround, confirming it wasn't caused by this change). Real
  Linux behaviour still pending CI's Linux leg per the acceptance criteria.
