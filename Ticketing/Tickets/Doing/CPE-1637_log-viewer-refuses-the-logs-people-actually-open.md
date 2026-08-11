---
id: CPE-1637
title: "The log viewer refuses every real incident log — a 256 KiB ceiling means CBS.log, dism.log and friends can't be opened at all"
type: Task
status: Doing
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

## Work Log (2026-08-11)

### Decision: bounded windowed read (tail-first), not streaming

Went with the ticket's first option — a bounded byte **window**, tail by default, paged backward on
demand — not the streaming (`ipc::Channel`) approach, for three reasons:

1. **Streaming solves the wrong problem here.** STREAMING.md's payoff is *painting first rows
   immediately* while a large listing/search keeps discovering more results over time. A log file's
   size is already known from `fs::metadata` before a single byte is read — there's nothing to
   "discover" — so streaming would add an `ipc::Channel`, a cancel/generation-token dance, and
   incremental UI state for a case that's really just "read a subset of known-fixed bytes." The
   existing `read_file_range`/hex-viewer paging precedent in this same codebase (CPE-772) already
   solves "page a huge file without loading it whole" with a plain `invoke` + Prev/Next, and a log
   viewer wants the same shape with one twist (default to the tail, not the start).
2. **The tail is what an incident actually needs.** Nobody opens `CBS.log` mid-failure to read line 1;
   they want the last few thousand lines. A bounded backward-paging window matches that directly, where
   a forward stream would by default paint the *oldest* content first and make the user wait for (or
   manually seek past) everything else to reach the part they actually came for.
3. **Smallest change that fully satisfies every non-negotiable.** Reusing the hex-viewer's seek+read
   shape means the new code is ~130 lines (`crates/server/src/log_window.rs`) plus a one-line dispatcher,
   not a new streaming plumbing layer, while still bounding work to `max_bytes` regardless of file size,
   never lying about what's shown, and keeping the three view-states distinct (see below).

### Implementation

- **`crates/server/src/log_window.rs`** (new) — `read_window(path, max_bytes, end)`: seeks straight to
  `end.unwrap_or(file_len).saturating_sub(max_bytes)` and reads exactly that span — never anything else
  in the file. `end: None` means "the tail"; passing a previous response's `window_start` back in as
  `end` pages further back. Trims a partial leading line (falling back to a raw UTF-8 char-boundary trim
  in the degenerate no-newline-in-window case, flagged via `line_aligned: false`) so a window never
  starts mid-line or mid-character — `\n` (`0x0A`) can never be a UTF-8 continuation byte, so line
  alignment and UTF-8 safety are the same fix. Returns `LogWindow { text, window_start, window_end,
  file_len, at_start, at_end, line_aligned }`. 11 unit tests: small-file/empty-file parity with the old
  whole-file behavior, a synthetic ~2.7 MB generated fixture (never a real machine path) proving the tail
  read stays byte-bounded, backward-paging continuity (no gap/overlap between adjacent windows), reaching
  byte 0, invalid-UTF-8 and missing-file error paths, and direct unit tests of the pure alignment step
  (`align_window`) including a synthetic split-multibyte-character case.
- **`src-tauri/src/lib.rs`** — new `read_log_window` command, thin `spawn_blocking` dispatcher into
  `cpe_server::log_window::read_window` (async per CPE-760/761 — every fs command on the main IPC thread
  must not block it), registered in both `generate_handler!` and the specta `collect_commands!` list.
  `read_file_text`/`read_file_text_impl` are untouched — the old all-or-nothing command still exists for
  every other preview provider; `PREVIEW_MAX_BYTES` was **not** raised, so a 15 MB read still can't reach
  any unrelated preview path.
- **`src/lib/preview/loaders.ts`** — added `loadLogWindow(path, end)`, mirroring the other preview
  loaders (reusable from the torn-off FloatPreview window too).
- **`src/lib/components/LogPreview.svelte`** — now fetches one `LogWindow` (tail-first) instead of the
  whole capped file. New state: `win` (the current window) and `pages`/`pageIndex` (a tail-first cache of
  windows visited this file-open). **Load earlier** reuses an already-fetched older page from the cache
  or issues exactly one more `read_log_window` call ending at the current window's aligned
  `window_start`; **Back to latest** jumps back to the cached tail page (`pages[0]`) — never a re-fetch.
  A new `<p data-testid="log-window-note">` states precisely which byte range is shown (see wording
  below), rendered only when the file didn't fit in one window (`!(at_start && at_end)`) — a small file
  behaves exactly as before, byte for byte, no new UI at all.
- **`src/docs/03-explorer.md`** — added a "Log preview" bullet under Files describing the severity
  highlighting/filtering (already shipped, previously undocumented — a pre-existing CPE-579 gap from
  CPE-1618, not introduced here) plus the new tail-window/paging behavior.
- **No new dependencies.** No `Cargo.lock` changes in either crate (verified via `git status`).

### Exactly what the UI tells the user

- Small file (fits in one window, unchanged from before): no note at all — identical to pre-CPE-1637.
- Tail of a big file: *"Showing the last 256.0 KB of this 15.4 MB file (bytes 15,133,362–15,395,506 of
  15,395,506)."*
- After paging back (neither the true start nor the tail): *"Showing bytes 14,873,362–15,133,362 of
  15,395,506 (15.4 MB total) — not the end of the file."*
- Reached byte 0: same wording, `at_start` disables **Load earlier**.
- Degenerate no-newline-in-window case: the above note plus *"This window has no line break in it, so it
  starts mid-line."*
- Never any wording implying the whole file was read when it wasn't, and never the old outright-refusal
  "File is too large to preview" message for a log opened through this path.

### Bounded WORK, not just output — confirmed three ways

1. **By construction**: `read_window` seeks to `raw_start` and calls `read_exact` for exactly
   `end - raw_start` bytes (≤ `max_bytes`) — there is no code path that reads, iterates, or measures
   anything outside that span. `fs::metadata` (an O(1) stat, not a read) is the only whole-file touch.
2. **By test** (`a_big_file_yields_a_tail_window_far_smaller_than_the_file`): asserts the returned
   window's byte span is `<= max_bytes` against a ~2.7 MB synthetic fixture.
3. **By measurement on a real 19.35 MB file** — see below.

### Measured open time (real file, this machine)

No `C:\Windows\Logs\CBS\CBS.log` at 15.4 MB on this machine today (it's 1.55 MB here — logs churn), but
`C:\Windows\Logs\DISM\dism.log` is **19,354,895 bytes (19.35 MB)** — the same file the ticket names, and
larger than its reported 19.2 MB. Measured directly by adding a throwaway
`crates/server/examples/measure_log_window.rs` (not shipped/committed as a test — a manual verification
tool per the ticket's "use real files to validate, don't depend on them in shipped tests" rule) that
calls `read_window` and times it:

```
cargo run --release --example measure_log_window -- "C:\Windows\Logs\DISM\dism.log"

file: C:\Windows\Logs\DISM\dism.log (19354895 bytes)
read_window(max_bytes=262144) took 14.3779ms
window: [19092915, 19354895) of 19354895 bytes (at_start=false, at_end=true, line_aligned=true)
returned text length: 261980 bytes
last line: 2026-08-11 09:11:49, Info  DISM  API: PID=32440 TID=31884 DismApi.dll: - DismShutdownInternal
```

**~14ms to open a 19.35 MB real log** (release build) — the tail window landed on the file's genuine
last line, confirming correctness as well as speed. No UI stall: this is a single `spawn_blocking`
seek+read, orders of magnitude under any perceptible-lag threshold, and — critically — this time is
**independent of file size** (bounded by `max_bytes`, not `file_len`), so the same ~14ms applies whether
the file is 19 MB or 19 GB. (The example file was removed before opening the PR — it isn't part of the
shipped crate.)

### Level detection/filtering on the visible window

Unchanged mechanism: `LogPreview.svelte` still calls `parseLog(win.text)` on whatever text the current
window holds, so severity detection/counts/filter chips operate on the visible window exactly as they did
on the old whole-file text — just now that "whole text" is a window instead of the entire file. Covered
by the full existing `LogPreview.test.ts` filter-chip suite (unchanged, still passing) plus new
CPE-1637-specific tests.

### Three-state distinctness (read-failure / empty / partial) — tests added

New `describe("LogPreview windowed/partial reads (CPE-1637)")` block in `LogPreview.test.ts` includes an
explicit `"a read failure, an empty file, and a partial windowed view remain three visibly distinct
states"` test asserting none of `log-load-error` / the empty note / `log-window-note` appear together
across the three scenarios.

### Tests

- `crates/server`: **11 new unit tests** in `log_window.rs` (all passing); full crate suite
  `cargo test --lib`: **1914 passed, 0 failed, 2 ignored** (pre-existing ignores, unrelated).
  `cargo test` (all targets, integration suites included): 0 `FAILED` occurrences.
- `npm run check`: **0 errors, 0 warnings** (after regenerating `src/lib/bindings.gen.ts` via
  `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` for the new `LogWindow`
  type + `readLogWindow` command).
- `npx vitest run`: **283 test files / 3495 tests, all passing** (full suite baseline before this ticket
  was 280 files / 3426 tests per CPE-1635's work log — this ticket's new/rewritten `LogPreview.test.ts`
  content plus other work merged since account for the difference). `LogPreview.test.ts` alone: 23 tests
  (13 pre-existing, rewritten to mock `readLogWindow` instead of `readFileText`, unchanged assertions;
  10 new for windowing/paging/state-distinctness).
- `cargo clippy --all-targets -- -D warnings` — clean in **both** `crates/server` feature modes tried
  (default, and `--features index`) plus the extractor combo (`--features
  pdf-thumb,video-thumb,waveform,dicom-thumb`), and in **both** `src-tauri` modes (default and
  `--features sidecar-platform`). All exit code 0, zero warnings.
- `cargo build` (both crates) — clean.

### Not verified

- No real machine copy of a 256 KiB–5,000-line file existed here either (same gap CPE-1618's tester
  found), so the pre-existing `linesCapped` note ("Showing the first N of M lines… of this window") is
  only exercised by the synthetic-fixture unit/vitest tests, not a real file — same situation as before
  this ticket, not a regression.
- No visual/screenshot verification of the new "Load earlier"/"Back to latest" buttons or the window
  note's layout — jsdom cannot see layout (per this crew's standing rule); only DOM/text assertions were
  possible here. A human/Visual-Critic pass on the actual buttons' appearance is outstanding.
- Did not attempt a genuinely multi-**gigabyte** file (none available on this machine) — the "bounded
  regardless of file size" claim rests on the seek+read code path having no size-dependent step (verified
  by reading the implementation) plus the 19.35 MB measurement, not a GB-scale empirical run.
