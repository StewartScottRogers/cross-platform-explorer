---
id: CPE-1803
title: a degraded trash listing says "Trash is empty", so unreadable is indistinguishable from empty
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-20
---

## Problem

CPE-1791 made the Trash view survive a malformed `.trashinfo` on Linux: instead of the whole view
dying with an opaque error, the listing **degrades to empty** and the restore/empty paths **fail
loudly**.

The backend half of that promise holds. The frontend half does not. A degraded listing renders
`"trash.empty": "Trash is empty"` (`src/lib/i18n.ts:902`) — the same string a genuinely empty trash
gets. So **an unreadable trash is indistinguishable from an empty one.**

That is precisely the misstatement CPE-1791's own test forbids for restore and empty
(`restore_and_empty_trash_fail_loudly_instead_of_reporting_false_success_when_the_dependency_panics`).
The rule was applied to two of the three surfaces and missed the one the user actually looks at first.

## Why it matters

The whole point of the scope reduction in CPE-1791 was that a smaller promise, **honestly kept**, beats
a larger one that lies. "Shows nothing and says so" was the deal. Right now it shows nothing and says
the wrong thing — a user with a trash full of files is told it is empty, and has no signal that
anything went wrong.

It is also the more dangerous direction of the two: someone who believes their trash is empty stops
looking for the file.

## What to do

- Give the degraded case its own state. A `trash.error` string already exists at `src/lib/i18n.ts:903`,
  so a distinct third state is cheap — the work is threading "the listing degraded" out of the command
  rather than inventing new copy.
- Decide what that state should say. It needs to convey that the trash could not be read, not that it
  is broken beyond repair — the entries are still there, and restore still works for anything the app
  can see. Keep it actionable; a user cannot fix a malformed `.trashinfo` from inside this app, but
  they can be told the listing is incomplete rather than empty.
- Add the matching sentence to `src/docs/38-trash.md`.
- **Red-proof it** per the Evidence Rules in `Ticketing/wiki.md`: a degraded listing must render the new
  state, and a genuinely empty trash must still render "Trash is empty". A test that only checks the
  first is half a test — the two states must be shown to be distinguishable.
- While in there, check the streamed path (`list_trash_stream`) reaches the same state as the
  collect-to-vec path. CPE-1791 covered both in the backend; the frontend should not diverge.

## Notes

Filed by the Foreman from PR #954's final verification, 2026-08-20. The verifier noted it explicitly as
out of that PR's scope (frontend was excluded) rather than letting it pass unmentioned — the right call,
and the reason this is a ticket rather than a surprise.

Related: **CPE-1791** (the backend degradation and its loud-failure rule), **CPE-1798** (the status bar,
another place a user reads what went wrong).

## Work Log

- 2026-08-20 — merged as **#957** (`21a56609`), batch 32. Two rework rounds.
- **The signal is variant-derived, never inferred from emptiness.** `degrade_panic_to_empty` returns
  `(Vec<TrashItem>, bool)` with the flag set only on `PanicCaught`; `list_trash` returns
  `TrashListing { entries, degraded }` and `list_trash_stream` resolves `TrashStreamSummary { count,
  degraded }` — the flag rides the resolved value because a degraded pass sends **zero batches, the same
  wire shape as a genuinely empty one**. All four zero-entry routes stay separable: empty → `trash.empty`,
  caught panic → `trash.degraded`, `trash::Error` → `trash.error`, `JoinError` → `trash.error`.
- Message: *"Trash couldn't be fully read — it may not be empty"*, in all 12 complete locales. It
  contradicts "empty" head-on, promises no action the user cannot take (a malformed `.trashinfo` is not
  fixable from inside this app), and avoids implying the trash is broken — entries are still there and
  restore still works for anything visible.
- **Rework 1** — the titlebar still rendered `"0 items"` beside the message denying the count. A hard
  number asserting emptiness inches from the sentence saying the number is unknown. Suppressed under
  `degraded`, red-proofed with an explicit `queryByText("0 items")).toBeNull()`.
- **Rework 2** — the first attempt reached for `var(--warn, #b5872b)` and bumped the hard-coded-hex ratchet
  from 466 to 467 to get past the guard. **`--warn` is not a theme token** — undefined repo-wide, so the
  fallback always wins — and `AgentTimeline.svelte:2102-2105` already labels that idiom deprecated. A fixed
  hex renders identically in both themes, so the warning would have been least legible in dark mode, where
  it most needs reading. Rebuilt on real semantic tokens; ratchet restored to 466 with **zero** new
  literals. The general fix is **CPE-1810**.
- Verification: the UAT proved distinguishability by **asymmetric mutation** — breaking the render
  condition redded the degraded test while the genuinely-empty test stayed green, which is what shows the
  pair distinguishes two states rather than both riding on one. It also test-merged against `main` and
  re-derived `bindings.gen.ts` rather than trusting the committed file.
- **Known remaining, filed not forgotten**: **CPE-1804** (a non-UTF-8 trash name is skipped silently, a
  second and *not* Linux-only route to the same lie), **CPE-1805** (degraded-with-entries renders no
  notice; unreachable today but CPE-1804's fix makes it normal), **CPE-1806** (the Linux-only test can
  `skip_notice!` itself, making its assertions vacuous under a green tick).
