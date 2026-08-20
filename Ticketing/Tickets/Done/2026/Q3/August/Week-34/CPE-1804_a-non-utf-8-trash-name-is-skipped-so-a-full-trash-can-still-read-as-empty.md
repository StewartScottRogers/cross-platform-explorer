---
id: CPE-1804
title: a non-UTF-8 trash name is silently skipped, so a full trash can still read as empty
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-20
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

## Work Log

- 2026-08-20 — merged as **#962** (`6dc2b749`), batch 36. Two rework rounds, one of them a real CI red.

### What shipped
`skipped: usize` on both `TrashListing` and `TrashStreamSummary`; new `trash.skipped{One,Many}` in all 12
complete locales. The **wording** splits, not the state: a per-item skip knows a number and gets one; a
caught panic lost the whole pass, has no number, and keeps CPE-1803's unquantified message. One shared
`listing_is_degraded(panic_degraded, skipped)` means the two routes cannot disagree, and a third route later
has one obvious place to join.

The decision to reuse `degraded` rather than add a second flag was deliberate: two flags mean every consumer
must remember to check both, and the first one to forget reintroduces this exact bug.

### The Linux red, which is the story
The first push was **green on Windows and red on ubuntu**: two new tests panicked inside
`trash-5.2.6/src/freedesktop.rs:350`, the very dependency panic CPE-1791 exists for. Fabricated `TrashItem`s
reached `trash::os_limited::metadata`, which on Linux derives the in-trash path via
`Path::new(id).parent().unwrap().parent().unwrap()` — and the fixture's bare `"id-ok"` has fewer than two
ancestors. On Windows the same call is a COM lookup that merely returns `Err`.

The reviewer had already flagged the PR's sentence *"the new tests touch no OS trash"* as false, and filed it
as a **wording** correction. **That false belief was the reason nobody looked.** The author's own account:
it concluded fabricated items never reach the OS and let a Windows-green run stand in for both platforms.

**The tempting fix was investigated and rejected on evidence.** A `catch_unwind` at that boundary would be
**unreachable from production input** — every real `id` comes from `list()` as a full
`<trash>/info/<name>.trashinfo` path (`freedesktop.rs:107`, `:121`), so both `.parent()` calls always
succeed. The verifier confirmed this by reading the dependency rather than the argument. It would have looked
like hardening while changing nothing, and left the real problem intact.

Instead the single OS call (`trash_item_size`) was split from the whole skip decision
(`trash_item_to_entry_with_size`), removing the Linux-runtime dependency rather than arguing with it. The
verifier confirmed `size` is **not load-bearing**: the `?` chain is `id?`/`name?`/`original_parent?`, and
`size` is forwarded unexamined to an infallible constructor — so no test stubs the thing under test.

### The tripwire — the most valuable thing in the change
`fabricated_trash_item_ids_satisfy_the_dependencys_path_preconditions` asserts the invariant the Linux
dependency unwraps. **Reverting the fixture to the original bare `"id-ok"` reds it on Windows**, so a defect
that previously only appeared on a Linux runner is now catchable before a push. Independently confirmed.

### Evidence gaps closed rather than documented away
- Nothing pinned that either command routed through `listing_is_degraded` — substituting
  `degraded: panic_degraded` left the whole suite green. Both command bodies moved into `list_trash_from` /
  `trash_stream_summary_from` with the outcome injected; the mutation now reds, at both sites and at the
  streamed site alone.
- The new assertions had come to depend on **the machine's ambient Recycle Bin contents** — a flake with a
  plausible-looking cause. Replaced with deterministic pins covering strictly more (clean/panic/skip/error ×
  both commands).
- The UAT's F1 — `skipped: 0` in both structs left 253 tests green — is **closed**, not annotated.

All four mutations were re-run independently by the verifier with the **green sets intact**.

### Known and not hidden
Recorded from the verifier: the round-2 evidence table says 198 where it is 199; a round-1 table row cites
real-OS assertions that the ambient-dependency fix deleted; the 3-line `list_trash_stream` adapter's flag
wiring is pinned nowhere; and `src/lib/i18n.ts:71` still claims "293 keys". Also **CPE-1816**: a partial
listing still renders as complete *while the stream is in flight*, because the flag rides on the summary.
