---
id: CPE-1803
title: a degraded trash listing says "Trash is empty", so unreadable is indistinguishable from empty
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
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
