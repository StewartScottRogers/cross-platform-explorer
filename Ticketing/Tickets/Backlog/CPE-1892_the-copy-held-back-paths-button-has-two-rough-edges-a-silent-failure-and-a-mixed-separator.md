---
id: CPE-1892
title: the copy-held-back-paths button has two rough edges — a silent failure and a mixed separator
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

CPE-1869's "Copy all N held-back paths" button (`src/lib/components/RevertOutcomePanel.svelte`)
shipped correct and gated correctly. Its independent reviewer flagged two non-blocking rough edges
on the way through; neither was worth holding the merge for, and both are worth closing.

**1. A failed clipboard write says nothing.** `RevertOutcomePanel.svelte:88-89` swallows a
`navigator.clipboard.writeText` rejection (`catch { /* clipboard unavailable */ }`). The button
simply never flips to "Copied". That fails *safe* — it never falsely claims success — but the only
signal the user gets is the absence of a confirmation they may not have been watching for. The
whole point of this affordance is that the user is about to go delete files by hand from a list
they cannot otherwise see; "I clicked it and nothing happened" is the worst possible state to leave
them in. Clipboard writes genuinely do fail: a non-secure context, a permissions policy, or focus
having moved off the document are all real.

**2. Mixed path separators on a Windows root.** `RevertOutcomePanel.svelte:74` strips a trailing
`\` or `/` from `root` and then joins with `/` only, so `C:\Users\joe` + `added-1.txt` copies out as
`C:\Users\joe/added-1.txt`. Cosmetic — most Windows tools accept it, and it does match the wire's own
`/`-joined convention — but it diverges from the OS-native separator, and this button is explicitly
modelled on Explorer's "Copy as path", which produces backslashes throughout. A user pasting into
`cmd`, a `del` line, or a text file they are reading by eye will notice.

## Acceptance criteria

- [ ] A failed clipboard write produces a visible, non-modal failure state on the button itself
      (not a dialog, not a toast that can be missed) telling the user the copy did not happen.
      Decide and record what it offers as a fallback — the paths are still on screen only 8 at a time.
- [ ] Join the root and each relative path with the platform's own separator, so a Windows root
      produces an all-backslash path and a POSIX root an all-forward-slash one. Reuse
      `formatPathsForClipboard`'s conventions rather than adding a second path-joining rule.
- [ ] Red-proof both: force the clipboard rejection and observe the failure state; feed a
      backslash root and assert on the produced payload. Observe red with the fix removed, record
      the line, restore.
- [ ] While in this harness, capture the missing evidence case CPE-1869's Visual Critic called out:
      an alias/collision (or retryable) hold-back with a **long, capped** list — "and N more" present
      but no copy button. That is the combination most likely to make the deliberate absence of the
      button read as an omission, and no screenshot covers it yet. Add it to the harness's fixtures
      so it is captured every run, not once.
- [ ] Do not touch the `advises_manual_delete` gating or the 8-name preview cap — both are settled
      by CPE-1869 and CPE-1845 respectively, and the alias/collision case must still show no button.

## Notes

Filed 2026-08-26 by CPE-1869's independent reviewer, which approved the PR while recording both.
Related: **CPE-1869** (the affordance itself), **CPE-1845** (the panel and the typed outcome),
**CPE-1823** (the stand-down that produces the hold-backs).

Note the standing limit for anyone testing this: jsdom applies no component CSS under this
project's vitest config, so nothing written there can check that the failure state is *visible*.
The real-browser harness `scripts/dev-harness/revert-heldback-copy/` already mounts this exact
component and is what CPE-1869's UAT drove — reuse it rather than asserting on markup.
