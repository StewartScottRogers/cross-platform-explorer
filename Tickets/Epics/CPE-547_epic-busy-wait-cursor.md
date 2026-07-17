---
id: CPE-547
title: "EPIC: Busy/wait cursor — show a spinner/hourglass when an operation runs long"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-16
---

## Summary
The app can *feel* frozen whenever a call to the Rust backend (or any async wait) takes longer than a
human expects an instant action to take — a large `list_dir`, a slow preview decode, a network/updater
round-trip, a sidecar/AI-Console request. There is no consistent signal that the app is *working*, so a
slow call is indistinguishable from a hang. This epic adds a **global busy-cursor affordance**: after an
operation has been outstanding past a short threshold, the mouse cursor switches to the platform's
wait indicator (spinner / hourglass / beachball) and reverts the moment the operation resolves.

## Goal
Every operation that can take "long enough to notice" gives the user immediate, uniform feedback that
the app is busy — **without** adding perceptible latency to fast operations. Applied **everywhere
possible**: ideally one shared mechanism wraps the invoke/async boundary so individual call sites get
the cursor for free, rather than each feature hand-rolling it.

## Rough scope (NOT decomposed — resolve at activation)
- **A single busy-tracking primitive** — a ref-counted "operations in flight" store/guard so overlapping
  slow calls are handled correctly (cursor stays busy until the *last* one finishes; no flicker).
- **Threshold, not immediate** — only flip to the wait cursor after a debounce (e.g. ~150–250ms) so fast
  calls never flash a cursor. Revert instantly on completion.
- **Wrap the invoke boundary** — a thin wrapper around Tauri `invoke` (and other async waits) that
  enters/exits the busy guard automatically, so "everywhere" is mostly free instead of per-call-site.
- **The visual** — CSS `cursor: progress` / `wait` at the app root (or an app-wide overlay class), using
  the native platform cursor so it's correct and familiar on Windows/macOS/Linux. Consider an in-app
  spinner for regions where the OS cursor alone isn't enough (e.g. a busy panel).
- **Coverage sweep** — audit every `invoke(...)` and long await in `src/` (dir listing, previews,
  search, updater, sidecar/AI-Console calls, Agent Watch reads) and confirm each is covered.
- **Escape hatch** — operations that already show their own progress (progress bars, streaming) should
  opt out so we don't double-signal.

## Precedence note
The plain explorer's PURPOSE tiebreaker is fast/small/predictable. This feature must not violate it:
the busy cursor is a *predictability* win (no more "is it hung?"), but the threshold/debounce must
guarantee **zero added latency and no flicker on fast calls**. If it can't be done without slowing the
common path, it isn't done.

## Open questions (resolve at activation)
- One global wrapper around `invoke`, or an explicit `withBusy()` helper call sites opt into? (Global is
  more complete for "everywhere"; explicit is more predictable.)
- Exact debounce threshold — one value app-wide, or per-operation-class?
- OS cursor only, or also an in-app spinner/overlay for slow regions? Where is each appropriate?
- How do streaming/long-running operations (agent sessions, updater downloads) opt out cleanly?
- Does this belong partly in the AI Console / sidecar host, or is it purely a frontend concern?

## Child tickets (created at activation)
- _None yet — this is a dormant brief. Decompose with `/ticketing-epic activate CPE-547`._

## Status
**Proposed.** A brief only; not researched, planned, or sub-ticketed until activated.

## Notes
Filed 2026-07-16 at the user's request: "add a mouse cursor that becomes active with a spinner or
hourglass if the call being made is too slow or the wait is too long — everywhere possible." The
"everywhere possible" instruction is why this is an epic (a cross-cutting sweep over every async call
site) rather than a single ticket.
