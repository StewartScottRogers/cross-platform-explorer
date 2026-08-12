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

## Work Log
2026-08-11 (sprint, Worker) — Implemented all four remaining items (A′, A″, B, B′):

- **A′ (decode UTF-16, not refuse it):** new `text_encoding::detect_encoding_at(bytes, base_offset)` —
  offset-aware sibling of `detect_encoding` (BOM only honored at true offset 0; NUL-lane parity computed
  from `base_offset + i`, not slice-relative `i`). `log_window::align_window` now routes a detected
  `Utf16Le`/`Utf16Be` window through new `decode_utf16_window()`, a code-unit-pair-aligned parallel to the
  byte-aligned UTF-8 path: strips a leading BOM (only when `raw_start == 0`), snaps to a 2-byte code-unit
  boundary via `raw_start`'s own parity, searches for the `0x000A` code unit to land on a clean line
  boundary (never trusts the UTF-8 path's 1-byte `already_aligned` peek, which means nothing for a 2-byte
  code unit), and decodes with `std::char::decode_utf16` (unpaired surrogates → U+FFFD, never a hard
  failure — matches the crate's "malformed input degrades gracefully" convention). No new dependency.
- **A″ (offset-aware sniffing):** `detect_encoding_at`'s `base_offset` parameter is exactly this fix — a
  window starting at an odd absolute file offset no longer flips LE/BE.
- **B (re-fetch on "Back to latest"):** `jumpToLatest()` in `LogPreview.svelte` now issues a real
  `loadLogWindow(path, null)` call instead of moving the pointer back to cached `pages[0]`, resets the page
  cache to just the fresh tail, and a new `fileGrew` reactive flag (comparing the refetched `file_len`
  against the file's size at open-time, tracked in `openedFileLen`) surfaces "The file has grown since this
  preview was opened" in the window note when true.
- **B′ (bounded page cache):** new `MAX_CACHED_LOG_PAGES = 20` and pure `pushLogPage()` helper in
  `logViewer.ts` — evicts the oldest/shallowest cached page once the cap is exceeded; `LogPreview.svelte`'s
  `loadOlder()` now calls it instead of the old unbounded `pages = [...pages, w]`. Cache is provably bounded
  at exactly 20 pages regardless of how many times "Load earlier" is clicked (`pushLogPage` test pushes 500
  pages and asserts `pages.length <= MAX_CACHED_LOG_PAGES` throughout, plus an exact-eviction-order test).

**Byte-level verification (independent of the committed test fixtures):** generated real UTF-16LE/BE files
with and without a BOM using .NET's own `[System.Text.Encoding]::Unicode`/`BigEndianUnicode` encoder (not
this crate's code) in a scratch directory, confirmed the raw bytes by hex dump (e.g. `edge_le_nobom.log`
first 16 bytes `5B-00-30-00-38-00-31-00-30-00-2F-00-31-00-32-00` — genuine interleaved-NUL UTF-16LE of
`[0810/12…`), then ran a temporary integration test calling the real `cpe_server::log_window::read_window`
against all four files: all four decoded byte-exact back to the original 3-line mixed INFO/WARNING/ERROR
text, confirmed `at_start && at_end` on the whole-file window. Temp verification files (Rust test +
generated fixtures) deleted before commit — the permanent coverage is the ticket-committed
`crates/server/src/log_window.rs` test suite (19 `log_window` tests including
`utf16le_tail_window_of_a_larger_file_decodes_correctly_even_when_the_window_lands_on_an_odd_byte_offset`
and `utf16_windowed_paging_never_reintroduces_a_split_code_unit_across_several_backward_pages`, both
exercising real windowed/paged reads over a larger synthetic file, plus `text_encoding`'s 26 tests
including the two offset-parity-flip regression tests), all passing.

Verification: `npm run check` clean; `npx vitest run` 287/287 files, 3660/3660 tests; `cargo clippy
--all-targets -- -D warnings` clean in all three of `crates/server`'s CI feature-mode combos (default,
`--features index`, `--features pdf-thumb,video-thumb,waveform,dicom-thumb`); `cargo test` in
`crates/server` — 19/19 `log_window` tests, 26/26 `text_encoding` tests, full crate suite green (no os
error 225 hit this run).

2026-08-11 (sprint, Worker, PR #842 review round 2 — F3, MOST SERIOUS finding of the whole review; F4) —
An independent reviewer of PR #842 ran this exact reproduction and it panicked:
```rust
let text = "2026-08-11 00:00:00 INFO user reacted with \u{1F600} to the message\r\nnext line\r\n";
let utf16le: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
fs::write(&f, &utf16le).unwrap();
read_window(&f, 4096, None).unwrap(); // Err("File is not valid UTF-8 text.")
```
**Root cause:** `classify_nul_bytes` (`crates/server/src/text_encoding.rs`, pre-existing logic this
ticket's A′/A″ work built on top of, not code this ticket wrote) required the "other" NUL lane to have
EXACTLY ZERO NUL bytes. The emoji's low surrogate (U+1F600 → LE code units `3D D8` `00 DE`) puts a `0x00`
on the lane that's otherwise all non-NUL ASCII bytes; that single stray NUL flipped the whole window's
classification to `Binary`, which then fell through to the byte-aligned UTF-8 path and failed outright.
Confirmed by the reviewer as WORSE than the pre-A′ state: before A′, a detected UTF-16 file got an honest
"looks like UTF-16, doesn't decode yet" refusal; after this bug, the same file silently misses UTF-16
detection entirely and gets a generic "File is not valid UTF-8 text" — indistinguishable from real file
corruption to a user already investigating a problem. Since this ticket's entire premise is "decode UTF-16,
don't refuse it — it's a common Windows log encoding" and real Windows logs routinely contain non-ASCII
(localised usernames, accented paths, smart quotes, emoji reactions in chat-adjacent logs), this was in
scope to fix here even though the underlying heuristic predates this ticket.

**Fix (`crates/server/src/text_encoding.rs`):** `classify_nul_bytes` now tolerates the minority NUL lane
being up to `UTF16_MINORITY_NUL_RATIO` (25%) NUL, instead of demanding exactly 0%. Threshold chosen from
measurement, not guessed — see the constant's doc comment for the full reasoning:
- Realistic mixed-ASCII UTF-16 log text (English lines with an occasional emoji/accented name/CJK
  username) puts at most ~1-2% of the minority lane's positions at NUL in practice (measured against
  `.encode_utf16()`-generated fixtures) — an emoji's low surrogate is the only realistic case that pollutes
  the "wrong" lane at all; accented Latin-1 (é, à) and BMP CJK/RTL characters all land with a zero *high*
  byte, i.e. on the SAME lane as ASCII, contributing no minority-lane noise at all.
- Real binary data that clears the *majority*-lane bar in the first place (unchanged `nul * 2 >= lane`
  check) pollutes both lanes together rather than concentrating on one. The worst real-world case found by
  scanning actual files on this machine was a Windows PE executable's zero-padded DOS/PE header
  (`notepad.exe`'s first 512 bytes): ~46% minority-lane NUL alongside ~53% majority-lane NUL. A handful of
  real PNG icons from this repo's own `src-tauri/icons/` measured only ~3-5% majority-lane NUL (nowhere
  near the 50% majority bar at all).
- 25% sits with wide margin above the realistic-text noise floor (~1-2%) and well below the binary floor
  found by measurement (~46%).

Verified BOTH directions with committed byte-level tests (not manual probing) in both `text_encoding.rs`
and `log_window.rs`: non-BMP surrogate pair (emoji) mixed with ASCII, accented Latin, CJK, and a BOM-less
BE file with RTL (Arabic) content all now decode/detect correctly as UTF-16; a real PNG signature + IHDR
header (from this repo's own icon) followed by pseudo-random compressed-data bytes, and a PE-header-shaped
buffer with ~50/50 NUL distribution across both lanes (modeling the real `notepad.exe` measurement), both
stay classified `Binary` — the widened tolerance does not turn real binary data into a false UTF-16 report.

Red-then-green, reviewer's exact repro (now committed as
`bom_less_utf16le_with_an_emoji_decodes_not_refused` in `log_window.rs`):
- **Red (before):** `thread '...' panicked at src\log_window.rs:574:45: UTF-16LE content with an emoji
  must decode, not be refused: "File is not valid UTF-8 text."`
- **Green (after):** test passes; `w.text` equals the original string exactly.

**F4 (test coverage breadth):** every UTF-16 fixture in the PR before this round was pure ASCII, so the
minority-lane tolerance was never exercised at all. Added, in both `log_window.rs` (full windowed-read
level) and `text_encoding.rs` (unit level): a non-BMP surrogate-pair fixture, an accented-Latin fixture, a
CJK fixture, and a BOM-less UTF-16BE fixture (every prior fixture was LE) — all with RTL content for the BE
case, for extra coverage breadth. Also committed the reviewer's ad-hoc-probed "surrogate pair bisected by
the window boundary" case as a permanent test
(`utf16le_surrogate_pair_bisected_by_the_window_boundary_degrades_to_replacement_character_not_a_panic`):
built as a single long line with NO newline (so line-boundary alignment doesn't simply discard the
orphaned fragment before it's ever decoded — the more common case where a `\n` follows shortly after the
bisection point), confirming the orphaned low surrogate degrades to `U+FFFD` with no panic, exactly the
documented trade-off in `decode_utf16_window`'s doc comment.

Verification: `npm run check` 0 errors/0 warnings; `npx vitest run` — 287 files / 3670 tests pass (no Rust
tests affected by the JS-side count; this round's Rust changes are additive-only there). `cargo clippy
--all-targets -- -D warnings` clean in all three CI feature-mode combos (default, `--features index`,
`--features pdf-thumb,video-thumb,waveform,dicom-thumb`). `cargo test` in `crates/server` — 1940 passed, 0
failed, 2 ignored (up from the 1929/2-ignored baseline; +11 new tests this round — 5 in `log_window`, 6 in
`text_encoding` — no regressions, no os error 225 hit this run).
