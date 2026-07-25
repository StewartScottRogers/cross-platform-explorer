---
id: CPE-1020
title: Apply / remove the Windows shell registration (HKCU glue)
type: feature
component: Backend
priority: medium
tags: needs-prereq
epic: CPE-712
created: 2026-07-24
closed: 2026-07-25
status: Done
---

## Summary
Third slice of CPE-712: the thin Windows glue that **executes** the plan from CPE-1019 against the real
registry. Writes every `RegEntry` under HKCU and deletes every `remove` key, all under `Software\Classes`
so no elevation is required. Idempotent (re-apply is a no-op-equivalent overwrite; remove tolerates
already-absent keys). Exposed as async `#[tauri::command]`s (`install_shell_integration` /
`uninstall_shell_integration`) that are one-line dispatchers into `cpe_server`, per the architecture seam.

Prereq: **CPE-1019** (the plan). Windows-only code path behind `#[cfg(windows)]`; other OSes get the
stubbed "not supported here yet" arm so the contract compiles cross-platform.

## Acceptance Criteria
- [ ] Applying the plan creates the on-folder / background / drive entries; "Open in CPE" appears in the
      real Explorer context menu; uninstall removes every key with no residue (verify with `reg query`).
- [ ] Idempotent install + tolerant uninstall (absent keys don't error); errors surface via the contract
      envelope, never panic.
- [ ] `winreg` dep confined to `#[cfg(windows)]`; clippy clean both feature modes.

## Work Log
- 2026-07-24 (PM take-on) — Filed. Blocked on CPE-1019's plan model; kept as pure-apply glue so all
  decision logic stays in the tested plan.
- 2026-07-25 — **Done.** Added `apply_entries` / `remove_keys` glue + public `install_shell_integration`
  / `uninstall_shell_integration` / `shell_integration_installed` to `cpe_server::shell_menu` (Windows-only
  behind `winreg = "0.52"`, added as a `[target.'cfg(windows)'.dependencies]` mirroring the `xattr` unix
  pattern; cross-platform stubs so the 3-OS contract compiles). Uninstall deletes only our `…\shell\CPE`
  verb subkeys — never the shared `Directory\shell` containers — and tolerates already-absent keys
  (idempotent). Wired three thin async `spawn_blocking` `#[tauri::command]`s (install/uninstall/installed;
  `install` resolves `current_exe`) and registered them in both `generate_handler!`/`collect_commands`
  lists. **Checks:** 9/9 server tests (new Windows round-trip runs under an isolated scratch hive, no real
  keys touched); clippy clean both feature modes; `src-tauri` compiles. **Independent review:** correct
  scoping of the delete (no collateral to other apps' verbs), idempotent uninstall, async-guardrail
  compliant — no findings. **UAT:** installed against real HKCU → `reg query` confirmed all three verbs +
  command subkeys; uninstalled → `reg query` "key not found" (no residue); `installed()` flips true→false.
  Next: CPE-1023 surfaces this as a Settings toggle (with GUI verify by right-clicking a folder).
