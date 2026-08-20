---
id: CPE-1804
title: a non-UTF-8 trash name is silently skipped, so a full trash can still read as empty
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

`stream_trash_entries` (`src-tauri/src/lib.rs:2268-2285`) silently skips any trash item whose name is
not valid UTF-8. If **every** item is skipped, the user gets `"Trash is empty"` with a full trash and no
indication anything was dropped.

This is the **same harm as CPE-1803, reached by a different route.** CPE-1803 gave the *panic* route its
own honest state (`trash.degraded`, "Trash couldn't be fully read — it may not be empty"). This route
still lies, and it does not need a malformed `.trashinfo` to trigger — an ordinary file with an
undecodable name is enough.

## Why it matters

Someone told their trash is empty stops looking for the file. That was the whole argument for CPE-1803,
and it applies here unchanged. The ticket is only half closed while this route exists.

Note this route is **not Linux-only**, unlike the panic CPE-1791/CPE-1803 addressed — a name that fails
UTF-8 decoding is reachable on more platforms, so this may in practice be the *more* likely path to the
lie.

## What to do

- Reuse the state CPE-1803 built rather than inventing a second one. `degraded` already means "this
  listing is incomplete"; a skipped-undecodable-name is exactly that. Check whether the existing message
  reads correctly for this cause too, or whether the two causes want distinct wording — **decide and say
  why** rather than defaulting to reuse.
- Cover **both** the collect-to-vec and streamed paths, as CPE-1803 did.
- Consider whether the skipped item should be *counted* — a "3 items could not be shown" is far more use
  than an unqualified warning, and CPE-1704 established a counting-contract precedent elsewhere in the
  codebase for exactly this reason.

## Evidence

Per the Evidence Rules in `Ticketing/wiki.md`. The two states must be **distinguishable**: an all-skipped
listing renders the incomplete state, a genuinely empty one still renders "Trash is empty". Break each,
watch it fail, restore. Half a test here is a test that certifies the bug.

## Notes

Filed by the Foreman from the independent review of PR #957, 2026-08-20. The reviewer flagged it as out of
CPE-1803's scope rather than letting it pass unmentioned.

**Independently confirmed by that PR's UAT**, which reached the same finding by a different route and added
two details worth having before starting:

- The skip covers `id`, `name` **and** `original_parent` — any one of the three failing UTF-8 drops the
  whole entry.
- The current behaviour is already **pinned by the repo's own test at `src-tauri/src/lib.rs:14769`**, so
  like CPE-1801 this is a deliberate change to a guarded behaviour, not an unguarded fix. Update the pin;
  do not delete it.
- The UAT's framing of the harm: *"on Linux, filenames are arbitrary bytes, so a trash holding only such
  files still renders 'Trash is empty', and a mixed trash silently under-counts."* The under-count is the
  half that is easy to miss — it does not need an all-undecodable trash to mislead.

Related: **CPE-1803** (the panic route, fixed), **CPE-1791** (the backend degradation), **CPE-1704** (the
counting contract).
