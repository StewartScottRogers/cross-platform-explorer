---
id: CPE-1635
title: "The Checkpoints dialog overflows the viewport at narrow window widths, despite max-width:95vw"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Seen by the Visual Critic in real (headless) Chrome while reviewing CPE-1600 (PR #826). **Pre-existing** —
the critic explicitly re-checked with the ordinary content scenario, no long strings involved, and
reproduced it identically, so CPE-1600's new failure rows are not the cause and did not worsen it.

## The gap
At a **420px** viewport width, `CheckpointDialog`'s own chrome — the header buttons and Close — pushes past
the viewport edge, defying the `max-width: 95vw` the dialog declares. The content area behaves; it is the
dialog's own header row that fails to shrink or wrap.

Worth fixing because Checkpoints is a **recovery** surface. A user reaching for it is usually already having
a bad day, and a control they cannot reach because it is off-screen is a bad time to discover a layout bug.

## Fix
- Make the header row shrink or wrap sensibly instead of forcing the dialog wider than its own `max-width`.
  The usual culprits: a `flex` row whose children have no `min-width: 0`, or a button group with
  `flex-shrink: 0` and no wrapping.
- Check the other dialogs for the same shape while you are there — if this pattern was copied, it is copied
  elsewhere. Report what you find rather than silently fixing a dozen files; that may deserve its own ticket.
- **Verify by looking in a real browser at narrow widths, in both themes.** jsdom cannot see layout — this
  defect existed precisely because nothing that runs in CI can see it. If CPE-1629's screenshot harness has
  landed by then, consider adding a narrow-width capture of this dialog so it cannot regress unseen.

## Acceptance criteria
- At 420px (and narrower), the dialog and all its controls stay within the viewport; no horizontal scroll.
- Real checkpoints and failed-attempt rows both still read correctly at that width, in both themes.
- Screenshot evidence in the work log, since no automated test asserts this today.

**Conflict surface:** `src/lib/components/CheckpointDialog.svelte` (and possibly sibling dialogs if the
pattern is shared). Small and self-contained.
