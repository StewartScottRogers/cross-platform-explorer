---
id: CPE-1790
title: the render guard never scans the confirm dialogs the docs claim it covers
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

`src/docs/03-explorer.md` states that "the confirmation dialogs for delete/extract/unlock/run-command"
are covered by the bidi escape guard. They are not scanned by it at all.

`ConfirmDialog.svelte` and `PasswordPromptDialog.svelte` take generic `title` / `message` props and
render `{message}` as raw body text. Neither file contains any `.name`/`.path`-shaped reference of its
own, so `isCandidateComponent` never matches, so neither is ever added to `REGISTRY` — and the guard
test iterates `Object.entries(REGISTRY)`, which means `findUnsafeRenderLines` is **never run on those
files**.

`CANDIDATE_PATTERN`'s vocabulary is `name|path|fullPath|oldName|cwd|root|folder|dir|target|fileName|
filePath|linkPath`. A component whose filesystem-derived text arrives as `message` is invisible to it.

## Why it is not currently a live bug

Safety is being provided by the **callers**, not the guard. `App.svelte:1457`, `:4199` and `:4501` each
wrap `item.name` / `entry.name` in `displaySafeName` before composing the message string, and all three
are correct today.

That is the whole problem: the protection is a convention nobody enforces. **If a future call site
forgets the wrap, CI stays green** — the guard has no view of these components, and the docs assert
they are covered, so a reviewer checking the documented behaviour would conclude the guard has it.

This is the same failure shape CPE-1768 closed one level up: coverage that looks like a rule but is
actually a hand-maintained habit.

## What to do

The value is in making the invariant enforceable, not in escaping something already escaped:

- Decide where the escape belongs. Either the dialog escapes on arrival (the "leaf escapes what it
  renders" model CPE-1760 confirmed for `MediaPlayer`), or the caller keeps the duty and something
  *checks* that it did. The first is more robust — a leaf cannot forget on someone else's behalf — but
  double-escaping a string the caller already wrapped must not produce `[[RLO]]`, so check
  `displaySafeName`'s idempotence before choosing it.
- Whichever way, make the components **visible to the guard**. Today they cannot be candidates at all.
  Note the guard's documented boundary: component-prop positions (`message={…}`) are deliberately never
  scanned — only `title=`/`aria-label=`/`alt=` DOM attributes and body text are. So this may need the
  membership rule to recognise a component by something other than its own identifiers, or the dialogs
  to render through a shape the scanner already sees.
- **Correct `src/docs/03-explorer.md` either way.** If the dialogs end up genuinely covered, the claim
  becomes true. If they stay caller-protected, the doc must say so, and the "Not yet covered" list must
  name them. A documentation claim that outruns the mechanism is how the next person stops looking.
- Red-proof it: a call site that forgets the wrap must fail CI. Per the Evidence Rules in
  `Ticketing/wiki.md`, show it red before the fix and green after.

## Notes

Filed by the Foreman from PR #939's review, 2026-08-19. The reviewer found it by deliberately probing
**outside** the shape the ticket's own audit used — that audit grepped only props literally spelled
`name`/`path`, which is exactly the class of miss that produced this gap in the first place.

Related: **CPE-1768** (the membership rule this extends), **CPE-1760** (the leaf-escapes-on-arrival
model), **CPE-1771** (a sibling guard whose stated coverage also outran its real coverage).
