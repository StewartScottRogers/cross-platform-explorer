---
id: CPE-1671
title: Folder-watch Undo reports plain success even when it skipped a delete it refused to make
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Found by the independent UAT on PR #854, and it is the exact risk **CPE-1666's own scope section named**:
*"a silent skip is arguably worse than the original bug — the user thinks all the files were handled."*

CPE-1666 made `undoFire` re-stat each recorded copy before deleting it and skip anything that now resolves
to a directory, so a swapped-in tree is no longer recursively destroyed. That half works and is verified.

But the only signal a skip happened is a devtools `console.warn`. `App.svelte`'s `undoWatchFire`
(`App.svelte:5686`) shows the same toast — `notice.watchUndone`, *"Undid: {rule}"* — whether or not
anything was skipped, because `undoFire` never throws for this case and returns nothing the caller can act
on. A real user sees an ordinary success message while something was quietly left in place.

This matches the pre-existing convention for that function's other warn-only paths (a refused delete, a
per-path failure), which is why the UAT graded it non-blocking and PR #854 merged on it. The convention is
the thing to fix.

## Scope

1. Give `undoFire` a return value that says what actually happened — how many copies were removed, how many
   were skipped and why (at minimum: refused by the backend consent gate, failed per-path, skipped because
   the path is no longer a regular file).
2. Have `undoWatchFire` report it: full success keeps the current toast; a partial undo says so, names the
   count, and — this is the point — does not claim the undo was complete when it wasn't.
3. Every user-visible string goes through `$t(...)` with keys in all 12 `COMPLETE_LOCALES`, per the
   convention CPE-1634 established and the regrowth guard enforces. That is most of the work in this ticket.

## Acceptance criteria

- [ ] An undo where one recorded copy has become a directory shows the user a message that says something
      was skipped, not a plain success.
- [ ] An undo where every copy is removed still shows the ordinary success message — no new noise on the
      common path.
- [ ] A backend refusal and a per-path failure are also surfaced rather than only warned to the console.
- [ ] New strings are translated in all 12 locales and pass the `showNotice` i18n regrowth guard.
- [ ] A test drives the partial case and asserts on the rendered notice text, not on a return value.
- [ ] Removing the new reporting turns that test red.

## Notes

Filed by the Foreman from the PR #854 UAT, 2026-08-12. The UAT verified the protective half properly: it
built a real directory tree at the recorded path, ran the undo, and listed the directory back off disk to
confirm all three files survived byte-for-byte — then stripped the guard and watched the tree get destroyed,
so the test is not vacuous.

Related: **CPE-1666** (the re-stat, merged in #854) and **CPE-1651** (the backend consent gate whose
refusals are one of the outcomes that currently go unreported).
