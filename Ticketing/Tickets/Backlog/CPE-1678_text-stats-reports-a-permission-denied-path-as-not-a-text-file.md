---
id: CPE-1678
title: text_stats reports a permission-denied path as "not a text file" — the right code with the wrong cause
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Found by the independent UAT on PR #860, and the reviewer's judgement was explicit: *"worth a low-priority
ticket, not a shrug."*

CPE-1673 item 4 fixed the error **code**: `list_dir`, `hash_file` and `text_stats` no longer flatten a
missing path to `Internal`, and — importantly — a permission-denied path is no longer misreported as
`NotFound`. That part is verified end-to-end through the real dispatcher, on a genuinely denied path created
with `icacls /deny`:

| method | missing | real | permission-denied |
|---|---|---|---|
| `list_dir`   | `NotFound` | `Ok` | `Internal` — *"Access is denied. (os error 5)"* |
| `hash_file`  | `NotFound` | `Ok` | `Internal` — *"Access is denied. (os error 5)"* |
| `text_stats` | `NotFound` | `Ok` | `Internal` — **"not a text file"** |

The wire-level classification is right in all three. But `text_stats`'s **message** tells the user their
file is not text, when the truth is that it could not be read at all. That is a wrong-cause report — the
same misdiagnosis family this chain of tickets has been closing twice already in this file, one layer down
from the code into the string.

## Why it matters

Low severity: message text only. No caller branches on it — the `Internal` vs `NotFound` contract is
correct and is what code keys off. But "we couldn't read it" presented as "it isn't text" is precisely the
shape this crew keeps filing: a confident wrong answer where an honest "don't know" belongs. A user seeing
it will go looking at the file's contents instead of its permissions.

## Scope

`crates/server/src/text_stats.rs` — distinguish "read failed" from "read succeeded and the bytes are not
text". The read error is already available at the point where the current message is produced; surface it
rather than collapsing every failure into the not-text branch.

Check the sibling previewers while there: any other place that answers a read failure with a content-shaped
verdict has the same bug.

## Acceptance criteria

- [ ] A permission-denied path through `text_stats` reports the access failure, naming the real cause, and
      still classifies as `Internal`.
- [ ] A genuinely non-text file still reports "not a text file" — the honest case must not regress.
- [ ] A missing path still reports `NotFound`.
- [ ] A test covers all three, driven through the real `Dispatcher` entry point rather than the helper.
- [ ] Removing the distinction turns that test red.

## Notes

Filed by the Foreman from the PR #860 review and UAT, 2026-08-12. `text_stats.rs` itself was untouched by
that PR — the classification code CPE-1673 added is correct, and this is the pre-existing message beneath
it.

Related: **CPE-1673** (the error-taxonomy work this came out of) and the recurring rule it keeps proving —
*"we don't know" must never look like a confident answer*.
