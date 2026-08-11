---
id: CPE-1644
title: "A UTF-16 log now opens into NUL-interleaved garbage instead of a clean error — and \"Back to latest\" shows a stale snapshot of a growing log"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Both found by the independent UAT of CPE-1637 (PR #835), which passed on its own terms — it opened real
multi-megabyte logs in under 20ms and paged losslessly through all 74 windows of a 19 MB file. These are the
edges it turned up.

## Part A — UTF-16 logs open as garbage (the more serious half)
`C:\ProgramData\Microsoft\EdgeUpdate\Log\MicrosoftEdgeUpdate.log` is a **real, 4.68 MB, UTF-16LE** Windows
log. Opening it now **succeeds** and shows unreadable text: roughly half literal NUL control characters
interleaved with the visible ones. Dumped bytes look like `\x00[\x000\x008\x00/\x001\x000\x00…` where the
file reads `[08/10…`.

Cause: `String::from_utf8` trivially succeeds on UTF-16LE ASCII, because each byte — including the embedded
NULs — is individually a valid single-byte UTF-8 codepoint. So the "is this valid UTF-8?" check passes and
nothing notices.

**This is pre-existing** (`read_file_text_impl` has the identical gap) — but it was previously *unreachable*
for a real UTF-16 log of this size, because the 256 KiB ceiling refused the file outright. CPE-1637 made
these files openable, so a latent bug became a visible one. That's worth stating plainly: the change didn't
introduce the flaw, it exposed it.

What a user sees is neither readable text nor an honest error — and garbage on screen **looks like their log
file is corrupted**, which is exactly the wrong impression to give someone already investigating a problem.

**Fix:** detect UTF-16 (BOM, or the NUL-interleaving heuristic) and either decode it properly or refuse it
with a clear, specific message naming the encoding. Decoding is much better if tractable — UTF-16 is a
common Windows log encoding, so refusing it still leaves a real gap. Whichever is chosen, it must be
structurally distinct from both the empty-file and the partial-window states.

## Part B — "Back to latest" doesn't return to the latest
`jumpToLatest()` is a pure cache-pointer move back to the page fetched when the file was opened; it **never
re-fetches** (confirmed: the backend call count stays at 2 after a page-back plus a jump-to-latest). On a log
that is actively being appended to while the preview is open, "Back to latest" therefore shows the snapshot
from open-time, not the current tail.

Not dishonest — the byte-range note stays internally consistent with the cached window — but a user watching
a live incident will reasonably read "Back to latest" as "the newest lines right now". Fresh opens are always
accurate; it's only the in-session cache that goes stale.

**Fix:** re-fetch the tail on "Back to latest" (cheap — the UAT measured sub-millisecond reads on a 19 MB
file), or relabel it so it doesn't promise currency it isn't delivering. Re-fetching is the better answer.
Consider whether the byte-range note should also show that the file has grown since opening.

## Update 2026-08-11 — Part A was fixed inside CPE-1637 (PR #835); scope narrows
The garbage-rendering half was serious enough to fix in that PR rather than defer. `log_window.rs` now calls
the crate's existing `text_encoding::detect_encoding` (already used by `inspect.rs`) and **refuses cleanly**
with an encoding-naming message, surfaced through `loadError` — structurally distinct from the empty and
partial-window states. A reviewer confirmed no false positives: `0xFF`/`0xFE` are never valid UTF-8 bytes at
any position, so a genuine UTF-8 window cannot coincidentally look like a BOM; a literal mid-file `U+FEFF`
reports `Utf8Bom` and is correctly ignored; and plain-ASCII windows at 64 different offsets plus a sparse
embedded NUL were never wrongly flagged.

**So what remains here is smaller, and two new items replace it:**

**A′ — decode UTF-16 rather than refusing it.** Refusing is honest but still leaves a common Windows log
encoding unopenable. The reviewer judged refuse-over-decode a legitimate *interim* answer: the module's
line-alignment relies on `0x0A` never being a UTF-8 continuation byte, and that guarantee doesn't extend to
UTF-16's 2-byte, endianness-dependent code units, so windowed UTF-16 decoding needs byte-pair-aligned seeks —
real work, not a quick fix. Worth doing properly.

**A″ — a misaligned window can report the wrong endianness (cosmetic).** `classify_nul_bytes` keys off
index-within-*slice* parity, not absolute file offset. A window starting at an **odd** absolute byte offset
splits a UTF-16 code unit, and the NUL lane flips — so the file is still detected as UTF-16 and still
correctly refused (never silently decoded), but may be labelled LE when it is BE, or vice versa. Reproduced
by the reviewer. Not a safety bug; fix by aligning the sniff window to an even offset, or by softening the
message to say "UTF-16" without claiming a byte order.

**B (the paging-cache half below) is unchanged and still open**, and gains one more item:

**B′ — the `pages` cache is unbounded.** `pages: LogWindow[]` in `LogPreview.svelte` grows without limit
(`pages = [...pages, w]`) as the user pages backward, so exhaustively paging through a very large file
re-accumulates the whole file in memory — a slower, opt-in version of the exact problem CPE-1637 existed to
fix. Bound it (a sliding window of pages, or evict the far end).

## Acceptance criteria
- A real UTF-16LE log either renders readable text or is refused with a specific, honest message — never
  NUL-interleaved garbage. Test with a genuine UTF-16 fixture, generated by the test rather than a
  machine-specific path.
- "Back to latest" reflects content appended since the file was opened, or its label no longer implies it
  does; a test covers the growing-file case.
- The three existing states (read failure / empty / partial window) stay structurally distinct — CPE-1637
  got this right and it must not regress.
- No regression to the sub-20ms open times or the lossless seam continuity the UAT verified.

**Conflict surface:** `crates/server/src/log_window.rs`, `src/lib/components/LogPreview.svelte`, and
possibly `read_file_text_impl` in `src-tauri/src/lib.rs` if the encoding fix is shared. Overlaps CPE-1636
(detection false positives) and CPE-1638 (filtering hides stack traces) — all three touch `LogPreview`, so
sequence them.
