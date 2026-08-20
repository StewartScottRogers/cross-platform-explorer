---
id: CPE-1816
title: a partial trash listing renders as complete while the stream is still in flight
type: bug
priority: Low
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

While a trash listing is **still streaming**, rows paint as batches arrive and the title bar asserts
"Trash 1 item", "Trash 2 items" — **with no incompleteness notice**. The notice and the count-suppression
appear only when the stream's summary resolves.

So for the duration of the stream, a partial list renders exactly as a complete one would.

## Why it is not simply a bug in CPE-1803/CPE-1804

It is **inherent to the streaming design**: the `degraded` / `skipped` flags ride on the *summary*, which by
construction arrives last. Entries stream over the channel; the verdict does not exist until the walk ends.
Unchanged since CPE-1560 and untouched by CPE-1803 and CPE-1804 — both of which fixed the *completed* case
correctly.

The window closes on completion, so this is materially milder than the bugs those tickets fixed: the user
is briefly under-informed rather than durably misinformed. On a small trash it is imperceptible.

## Why it is still worth fixing

The harm scales with the thing that makes streaming worth having. A large or slow trash — a network mount,
a spinning disk, thousands of entries — is exactly when the window is widest **and** when the user is most
likely to read a partial list and act on it. "Your file isn't in the trash" is a conclusion someone can
reach in two seconds from a list that is still filling.

## What to do

- The honest shape is probably to **let the stream carry incompleteness as it happens** rather than only in
  the summary — the walker knows it skipped an entry at the moment it skips it. Weigh sending it with the
  batch against the simplicity the current design buys, and **say why** either way.
- Failing that, consider suppressing the *count* until the summary resolves, so the app at least never
  asserts a total it cannot yet back. That is strictly weaker but much cheaper.
- Whatever the shape, keep the property CPE-1804 established: the notice must be driven by the flag, never
  inferred from `entries.length`.
- Follow the streaming standard in [docs/design/STREAMING.md](../../../docs/design/STREAMING.md), including
  supersession by generation token — a fix here must not resurrect a stale banner over a fresh listing.

## Evidence

Per the Evidence Rules in `Ticketing/wiki.md`. The test must observe the **mid-stream** state, not just the
resolved one — a test that only asserts the final render passes today and would pass with this unfixed.

## Notes

Filed by the Foreman from PR #962's UAT, 2026-08-20. The UAT observed the timeline directly rather than
inferring it, and correctly classified it as pre-existing rather than a regression in the PR it was testing.

Related: **CPE-1803**, **CPE-1804**, **CPE-1805**, **CPE-1560**.
