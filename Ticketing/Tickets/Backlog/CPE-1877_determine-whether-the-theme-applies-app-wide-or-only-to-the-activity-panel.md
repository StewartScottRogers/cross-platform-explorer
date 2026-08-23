---
id: CPE-1877
title: determine whether a theme change applies app-wide, or only to the Activity panel
type: task
priority: Medium
status: Backlog
tags: needs-decision
estimate: S
created: 2026-08-23
closed:
---

## Observation — stated as an observation, not a diagnosis

While reviewing PR #1009's screenshots as the visual leg, the Foreman noticed that in the
**hc-dark** capture the Activity panel renders dark while the **main explorer window beside it stays
light** — light background, dark text, light sidebar. In the **light** capture of the same surface,
both are light.

Evidence, in the UAT's worktree
(`.claude/worktrees/uat-1009/gui-smoke/.screenshots/`, reproducible from PR #1009's branch):

- `cpe1821-agent-timeline-history-light.png` — main window light, Activity panel light
- `cpe1821-agent-timeline-history-hc-dark.png` — main window **light**, Activity panel **dark**

Same for the `cost`, `radar` and `live` captures at their `hc-dark` / `dark` variants.

## Two explanations, and this ticket is to determine which

1. **Harness artifact (likely).** The UAT's own spec may set `data-theme` on the panel's root element
   rather than on the document, so only the panel restyles. That would make this a defect in a test
   spec, not in the app, and the fix belongs in the spec.
2. **Real defect.** The theme genuinely does not propagate to one of these surfaces — e.g. the
   Activity panel is a separate webview or an overlay whose root never receives the attribute, or the
   main window's root does not. If so, a user switching to dark or high-contrast gets a half-themed
   app, which is a visible bug on a shipped feature.

**Do not fix anything until you know which.** Determining it is the whole ticket; the fix that follows
is small either way.

## Why this is filed rather than settled

The Foreman spotted it in screenshots taken for a different purpose and could not resolve it from the
images alone. Filing an observation with its evidence is honest; asserting a bug that turns out to be
a test artifact — or dismissing a real one — is not. CPE-1821 itself was unaffected either way: its
tokens resolve correctly per theme, which is what that PR changed, so it merged.

## What to do

1. Run the app for real and switch theme through the Settings selector — light, dark, both
   high-contrast. Look at the main window and the Activity panel together. Screenshot both.
2. If the app is fine, fix the gui-smoke spec so its theme switch is applied the way the app applies
   it, and say so — a spec that themes a subtree will keep producing misleading screenshots for every
   future visual review.
3. If the app is not fine, fix it and add a spec that would have caught it: assert the *main window's*
   computed background changes with the theme, not just the panel's.

## Note on running gui-smoke locally

The drivers are present (`tauri-driver.exe`, `msedgedriver.exe`, `msedgedriver-tool.exe` in
`~/.cargo/bin`) — do not install anything. But `msedgedriver` is version **150** against installed
Edge **151**, and the mismatch hangs longer sessions ("Timed out receiving message from renderer").
That is recorded in `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` with the suggested fix.

## Acceptance criteria

- [ ] A definite answer, with fresh screenshots, on which of the two explanations holds.
- [ ] Whichever it is, fixed.
- [ ] A test that fails if a theme switch ever again leaves one surface unthemed.

## Work Log

- **2026-08-23 16:20 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from its own read of PR #1009's screenshots while acting as the visual leg. Note the Visual Critic
  role normally does this looking; on this run it had no screenshots to read until #1009's UAT
  produced the first ones.
