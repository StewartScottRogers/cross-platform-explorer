---
id: CPE-1637
title: "The log viewer refuses every real incident log — a 256 KiB ceiling means CBS.log, dism.log and friends can't be opened at all"
type: Task
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent UAT tester of CPE-1618 (PR #829), testing against **13 real log files** off this
machine rather than the committed fixture. The viewer works well on small-to-medium logs. It cannot open the
ones a person actually reaches for during an incident.

`read_file_text_impl` (`src-tauri/src/lib.rs:1418`) checks file size **before reading any bytes** and returns
`Err` outright above 256 KiB — it does not truncate-and-continue. Measured, on real files:

| File | Size | What the user sees |
|---|---|---|
| `C:\Windows\Logs\CBS\CBS.log` | 15.4 MB | "File is too large to preview (15395506 bytes; limit 262144)." |
| `C:\Windows\Logs\DISM\dism.log` | 19.2 MB | same |
| `C:\ProgramData\Claude\Logs\cowork-service.log` | 13.4 MB | same |
| `...\QuickShareServiceLog.log` | 349 KB | same |

**This is honest, not misleading** — exact byte counts, no silent truncation, and it matches the preview
ceiling every other provider enforces (CPE-1618's own Scope said to reuse it). So it is not a regression and
did not block that ticket. But the consequence is that the "Showing the first 5,000 of N lines" note the
feature was designed around essentially **never fires**: it needs a file simultaneously *under* 256 KiB and
*over* 5,000 lines, a combination the tester could not find anywhere on a real machine.

Net: a log viewer that declines the logs worth viewing.

## Goal
Make the viewer usable on multi-megabyte logs, without loading them whole and without lying about what is
shown.

## Fix — options worth weighing (decide and log the reasoning)
- **Read a bounded window rather than refusing.** Read the **tail** by preference — for a log, the end is
  almost always what you want — with a clear "showing the last N KB of a 15.4 MB file" note and a way to
  page further back. This is probably the smallest change that makes the feature real.
- **Stream it** per [docs/design/STREAMING.md](../../docs/design/STREAMING.md), which this repo already uses
  for directory listings and search: paint the first rows immediately and append. That is the
  architecturally consistent answer and matches [[prefer-streaming-liveness]].
- Whatever is chosen, **do not** raise the shared `PREVIEW_MAX_BYTES` for every provider as a shortcut —
  that would push a 15 MB read into unrelated preview paths.

## Non-negotiables
- **Bound the WORK, not just the output.** A crafted font once froze this app 8.8 seconds because a cap
  counted items emitted rather than examined; the same trap applies to a huge log.
- **Never silently truncate.** Whatever window is shown, say so precisely — this viewer's honesty on that
  point is currently its best property, and it must survive the change.
- Keep the existing structural distinctness: read-failure, empty file, and truncated-view must remain three
  visibly different states.

## Acceptance criteria
- A multi-megabyte real log opens and is readable, with an accurate statement of which part is shown.
- Measured open time on a 15 MB log recorded in the work log; no UI stall.
- Level detection and filtering still work on the visible window.
- The three failure/empty/partial states stay distinct; tests cover each.

**Conflict surface:** `src/lib/preview/logViewer.ts`, `src/lib/components/LogPreview.svelte`, and — if the
read path changes — `src-tauri/src/lib.rs`'s `read_file_text_impl` and/or a new streaming command.
Coordinate with CPE-1638 (stack-trace grouping), which touches the same component.
