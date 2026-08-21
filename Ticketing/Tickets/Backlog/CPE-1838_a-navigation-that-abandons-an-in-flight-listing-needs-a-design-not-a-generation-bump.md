---
id: CPE-1838
title: a navigation that abandons an in-flight listing needs a design, not a generation bump
type: bug
priority: Medium
status: Backlog
tags: big-design
estimate: L
created: 2026-08-20
closed:
---

## Problem

Five places navigate away while a directory listing may still be streaming: `loadPath`'s HOME
short-circuit, `navigateB`'s HOME short-circuit, `enterArchive`, `openSmartFolder`, and
`openStructuredSearch` (`src/App.svelte`). None of them supersedes the in-flight load, so its later
batches can land in whatever the pane is showing next.

Only **one** of the five leaks visibly today — `enterArchive` followed by Up at the archive root, since
`exitArchive()` sets `archive = null` and returns without reloading. The other four are masked because
Home swaps `FileList` for `HomeView`, and the archive / smart-folder / structured-search overlays cover
`entries`. As CPE-1780's own text puts it: *"invisible today only because Home renders HomeView… that is
luck, not a guard."*

## Why this is a design ticket and not a patch

CPE-1780 attempted this as an exported `invalidateListing()` that bumped the generation counter. Split
out of that PR after the mechanism produced **three** distinct regressions across two review rounds, each
found by measurement:

1. **Stuck pane.** `loadListing`'s `finally { if (gen === loadGen) loading = false }` no longer matched
   after a bare bump, so a smart folder opened mid-load rendered "Loading…" forever —
   `FileList.svelte` puts `{:else if loading}` ahead of the entries branch, and none of the three exit
   functions reloads.
2. **Cancellation defeated (CPE-665 regression).** The cancel target was derived as `gen - 1`, which was
   only ever correct because every bump used to start a stream. A bare bump burned a generation no stream
   owned, so the next load cancelled an id that never existed while the real walk ran to completion.
   Measured: `list_dir_stream 1, cancel 1, list_dir_stream 2, cancel 3, list_dir_stream 4` — stream 2
   never cancelled.
3. **A confident lie.** Settling the pane inside the helper published `entries = []` and `error = ""` as
   a *finished, successful* listing. Deterministic, no race needed:
   `ERR-BEFORE = "Can't open this folder — permission denied."` → `ERR-AFTER = "This folder is empty"`.
   And an abandoned in-flight load returned to a pane reading "This folder is empty" where before the
   change the load completed and the rows were there.

Fix 2 was correct and is retained in CPE-1780 (an explicit `lastStreamId` rather than a derived one).
Fixes 1 and 3 are the pattern: `loadListing`'s `entries = []` / `error = ""` are only safe because a load
always follows them, so any helper that settles the pane *without* a load behind it publishes a lie.

## What a correct design has to handle

- Supersede the in-flight load **and** leave the pane in a state that is true — not stuck, not falsely
  empty, not falsely error-free.
- Cancel the real backend stream (`docs/design/STREAMING.md`, CPE-665), not a derived id.
- Say what happens on **return**. `exitSmartFolder`, `exitStructuredSearch` and `exitArchive`
  (`src/App.svelte:1832`, `:1868`, `:1956`) do not reload today. The Reviewer's smallest correct shape was
  to have them re-load pane A (`loadPath(currentPath, true)`, cache-served so instant), which also
  re-arms the stale-while-revalidate that a bare bump discards. Note `exitArchive()` is also called from
  `switchWorkspace` (`:1656`) immediately before its own `loadPath`, so an added reload must be harmless
  there.
- Cover pane B, not just pane A.

## Acceptance criteria

- [ ] The design is written down and agreed before implementation — that is the first deliverable.
- [ ] All five call sites supersede correctly, and each is tested for BOTH directions: the stale rows do
      not leak, **and** a legitimate listing still completes normally afterwards.
- [ ] The pane's state on return is true in every case: a real error survives, a real listing survives,
      and "empty" is only shown when the folder is genuinely empty and the load genuinely finished.
- [ ] The real in-flight stream is cancelled; no cancel targets an id no stream owned.
- [ ] Pinned by tests that fail if any of the three regressions above is reintroduced. Each is
      reproducible with a probe: pending `list_dir_stream` + invalidate + resolve, and the error-erasure
      case needs no timing at all.

## Notes

Split out of CPE-1780 by the Foreman under a stated boundary: the unreadable-entry count (F3) was
independently valuable and shipped, while this mechanism needed a design pass rather than a fourth patch.

The UAT's measurement of which sites leak observably is the right scoping input — fix the mechanism
properly, but know that only `enterArchive` + Up is user-visible today, so this is not urgent, it is
fiddly.
