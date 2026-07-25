---
id: CPE-1023
title: In-app "Shell integration" settings toggle
type: feature
component: Frontend
priority: medium
tags: needs-prereq
epic: CPE-712
created: 2026-07-24
closed: 2026-07-25
status: Done
---

## Summary
CPE-712 slice: the user-facing control. A toggle in **Settings** (per the avoid-modal-permission-popups
rule — no launch-time consent modal) that adds/removes the "Open in CPE" context-menu integration by
calling `install_shell_integration` / `uninstall_shell_integration` (CPE-1020). Reflects current state
(query whether the entries exist), shows a plain-language description, and reports success/failure inline.
Uninstall path is the reversibility guarantee made reachable from the UI.

Prereq: **CPE-1020** (the apply/remove commands). Cross-platform-safe: on OSes whose apply glue isn't landed
yet, the toggle is shown disabled with a "coming to <OS>" note rather than hidden.

## Acceptance Criteria
- [ ] A Settings toggle installs/removes the shell integration via the backend commands; state is read back
      and reflected on open.
- [ ] Failure surfaces inline (no crash, no modal-permission popup); copy is plain-language.
- [ ] Docs updated per CPE-579 (settings section → doc slug) if a new settings section is added.

## Work Log
- 2026-07-24 (PM take-on) — Filed as the UI cap on the Windows path (CPE-1019 → 1020 → 1023). Placed in
  Settings, not a launch modal, per [[avoid-modal-permission-popups]].
- 2026-07-25 — **Done (implementation + headless verification).** Added `ShellIntegration.svelte` — a
  self-contained Settings section (mounted in `SettingsDialog`, like `SidecarManager`) that queries
  `shell_integration_installed` on mount and flips via `install_/uninstall_shell_integration` (CPE-1020),
  re-reading state after each change (and after an error) so the checkbox never lies. Off Windows the
  control is shown **disabled** with a "coming to <OS> soon" note (discoverable, never calls the backend).
  `invoke` from `src/lib/invoke.ts` (busy-cursor wrapper). Docs: added a "Shell integration" section to
  `src/docs/03-explorer.md` (no `sectionDocs.ts` change — Settings isn't a nav `Section`). **Checks:**
  svelte-check 0 errors; full frontend suite **950/950** incl. 3 new component tests (check→install,
  uncheck→uninstall, off-Windows→disabled+no-calls). **Independent review:** invoke-convention, state
  correctness, cross-platform degrade — no findings.
- 2026-07-25 — **Verify note (honest gap):** the toggle's behaviour is proven headlessly and the commands
  it drives were live-registry-proven in CPE-1020 (`reg query`). The remaining confirmation — build →
  install the sidecar release → open Settings → flip it → right-click a folder and *see* "Open in
  Cross-Platform Explorer" — was **not** done here because it entails cutting a public release (an
  outward-facing action to leave to the user). Recommended as the closing visual check for CPE-712.
