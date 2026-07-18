---
id: CPE-662
title: "EPIC: Streaming liveness — progressive middle-pane loading"
type: Task
status: Proposed
priority: High
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal

Make the **middle pane feel instantly alive** when opening a folder. Today `open a folder` blocks on a
single `list_dir` round-trip that reads the **entire** directory into one `Vec<DirEntry>` and returns it
all at once (`App.svelte:743` → `invoke("list_dir")`), so the pane shows nothing until the *slowest*
entry has been stat-ed. On a big or slow folder (network share, huge photo dir, cold cache) that is a
visible dead wait with the busy cursor up.

Replace the one-shot load with a **streamed / chunked** listing: the backend emits entries in batches
over a Tauri **Channel** (or event) as it walks the directory, and the frontend paints the first rows
the instant the first batch arrives, filling in progressively. The pane should show *something real*
within a frame or two regardless of folder size, and stay scrollable while the rest streams in.

Make this the **general pattern** for incoming information, not a one-off: the user's standing guidance
is to prefer streaming/chunked delivery over blocking round-trips **wherever a payload can be large or
slow** — directory listings first, then search results, `read_archive_entries`, thumbnails, and any
future bulk producer. See [[prefer-streaming-liveness]].

## Why

This is the plain explorer's **core interaction** — opening folders — so the fast/small/predictable
tiebreaker applies at full force: a file explorer that stalls on open feels broken. Streaming turns a
"wait, then everything" experience into "immediately something, then more", which is the single biggest
perceived-performance win available. It is additive to correctness (same entries end up listed) and
degrades gracefully (a small folder still arrives in one batch).

## Rough scope (areas, not child tickets)

- **Backend streaming `list_dir`:** a command that takes a `Channel<Vec<DirEntry>>` (Tauri v2 channels)
  and pushes batches as it reads the dir, preserving the existing skip-unreadable-entries behaviour and
  the sort contract. Keep the current synchronous `list_dir` for callers that need a full vec (tests,
  archive expansion) or provide a compatibility shim.
- **Frontend progressive render:** replace the `await list_dir` at `App.svelte:743` with a subscription
  that appends batches to the `entries` store; drop the busy-cursor-until-done model in favour of a
  lightweight "still loading" affordance that doesn't block interaction. Handle mid-load navigation
  (cancel/supersede an in-flight stream when the user changes folders).
- **Ordering & sorting under streaming:** decide how sort interacts with incremental arrival (sort each
  batch as it lands vs. insert-sorted vs. re-sort on completion) so rows don't visibly jump.
- **Generalise the pattern:** apply the same channel-streaming shape to other large/slow producers
  (search results already partly stream — align them), and document it as the house style for bulk data.
- **Perf guardrail:** with a *small* folder, the streamed path must be no slower and no larger than the
  one-shot path — the delete-test equivalent for liveness.

## Open questions (resolve at activation)

- Batch size / cadence: fixed count (e.g. 200 entries), time-sliced (flush every N ms), or adaptive?
- Sorting: stream in on-disk order and sort client-side as batches land, or have the backend sort and
  stream sorted runs? How to avoid row-jump while keeping the final order correct?
- Cancellation: how does an in-flight stream get superseded when the user navigates away mid-load —
  drop the channel, a generation token, or an explicit cancel command?
- Do we keep the blocking `list_dir` as a public command (for tests / archive views), or route
  everything through the streaming one with a "collect to vec" helper?
- Does the virtualised list (if any) already cap render cost, or is stat latency the sole bottleneck?
  (Measure before building — the fix may be backend I/O concurrency as much as transport.)

## Definition of Done

- Opening a large/slow folder paints its first real rows within a couple of frames, then fills in
  progressively without freezing scroll or input.
- The busy-cursor "dead wait" on folder open is gone for the common case.
- Unreadable entries are still skipped (never fail the whole listing); final contents & sort order match
  the old one-shot `list_dir` exactly.
- Navigating away mid-load cleanly supersedes the in-flight stream (no stale rows bleed into the new
  folder).
- Small folders are no slower/larger than before (measured) — the plain explorer stays fast & predictable.
- The streaming pattern is documented as the standard for large/slow payloads and applied to at least
  one other producer beyond `list_dir`.
