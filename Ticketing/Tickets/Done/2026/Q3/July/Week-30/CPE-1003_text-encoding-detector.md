---
id: CPE-1003
title: Text-encoding + line-ending detector
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-1002
---

# CPE-1003 — Text-encoding + line-ending detector

## Summary

The pure engine for epic CPE-1002 ("File inspection & safety utilities"): a byte-slice sniffer that
guesses a text file's character encoding (empty / UTF-8 / UTF-8-BOM / UTF-16LE-BOM / UTF-16BE-BOM /
Latin-1 / binary), plus a separate `&str` scanner that reports which line-ending convention(s) a
decoded file uses (LF / CRLF / CR / mixed). No filesystem I/O, no new dependencies — the caller
supplies the bytes / string.

New module `crates/server/src/text_encoding.rs`.

## Design

- `pub enum EncodingGuess { Empty, Utf8, Utf8Bom, Utf16Le, Utf16Be, Latin1, Binary }`
  (`Debug, Clone, Copy, PartialEq, Eq`), with `label(self) -> &'static str`.
- `pub fn detect_encoding(bytes: &[u8]) -> EncodingGuess` — bounds-checked, never panics. Order:
  empty → `Empty`; BOM (`EF BB BF` / `FF FE` / `FE FF`) → `Utf8Bom` / `Utf16Le` / `Utf16Be`; valid
  UTF-8 → `Utf8`; else a binary heuristic (NUL byte anywhere, or >30% non-text control bytes in a
  512-byte sniffed prefix, excluding tab/LF/CR) → `Binary`; else → `Latin1` (guess of last resort).
- `pub enum LineEnding { Lf, Crlf, Cr, None, Mixed }` and
  `pub struct LineEndingReport { pub crlf: usize, pub lf: usize, pub cr: usize, pub mixed: bool,
  pub dominant: LineEnding }`, plus `pub fn detect_line_endings(text: &str) -> LineEndingReport` —
  counts CRLF pairs, lone LF, lone CR; `mixed` = more than one of the three counts is non-zero;
  `dominant` = the most common individual convention, tie-broken deterministically
  (`Crlf` > `Lf` > `Cr`), `None` when there are no line breaks.
- Pure std, zero new dependencies. `pub mod text_encoding;` added to `lib.rs` with a doc comment.

## Acceptance Criteria

- [x] `detect_encoding`: empty → `Empty`; ASCII and non-ASCII UTF-8 → `Utf8`; UTF-8-BOM, UTF-16LE-BOM,
  UTF-16BE-BOM → their respective variants; NUL/mostly-control invalid-UTF-8 bytes → `Binary`; an
  isolated high byte (invalid UTF-8, not binary-looking) → `Latin1`.
- [x] `detect_encoding` is bounds-safe and never panics on any length, including empty and 1–3 byte
  BOM-prefix-length input.
- [x] `detect_line_endings`: pure LF, pure CRLF, a CRLF/LF mix (`mixed=true`, exact counts), lone CR
  (old-Mac style), and no-line-breaks (`None`) are all covered with exact count assertions.
- [x] A `\r\n` pair is counted once as `crlf`, never double-counted as a lone `cr`.
- [x] Zero new dependencies; pure over a byte slice / `&str`, no filesystem I/O.
- [x] `pub mod text_encoding;` declared in `lib.rs` with a doc comment.
- [x] `cargo test --lib text_encoding` passes (19 tests).
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo clippy --all-targets --features index -- -D warnings` clean.

## Work Log

- 2026-07-24 — Built `text_encoding.rs` end-to-end: `EncodingGuess`/`detect_encoding`,
  `LineEnding`/`LineEndingReport`/`detect_line_endings`.
  - **Binary-detection heuristic:** two OR'd signals over bytes that have already failed the
    `std::str::from_utf8` check (see limitation below) — (1) a NUL byte *anywhere* in the input
    (cheap `bytes.contains(&0)`, no prefix cap — NUL is the strongest single binary signal and worth
    the full scan), or (2) more than 30% of a leading 512-byte sniffed prefix being non-text control
    bytes (`b < 0x20` excluding tab/LF/CR, plus DEL `0x7F`). The 512-byte cap keeps the check
    effectively O(1) on large input (mirrors the "sniff a prefix" approach tools like `git`/`file`
    use for binary detection) — bumping past it just costs a linear scan, no correctness issue.
  - **Latin1 fallback:** anything that is (a) not empty, (b) has no recognised BOM, (c) is not valid
    UTF-8, and (d) doesn't trip the binary heuristic falls through to `Latin1`. This is explicitly a
    guess of last resort, not a positive identification — byte-level sniffing alone can't distinguish
    Latin-1 from Windows-1252 or other 8-bit encodings; the label says "(guessed)" for exactly this
    reason.
  - **Ordering limitation (by design, not a bug):** the UTF-8-validity check runs *before* the binary
    heuristic, per the ticket's specified check order. Because a NUL byte (`0x00`) is itself valid
    UTF-8 (`U+0000`), input that is otherwise plain ASCII/UTF-8 with embedded NULs — e.g. no-BOM
    UTF-16LE ASCII text (`h\0i\0…`) — still passes `str::from_utf8` and is reported as plain `Utf8`,
    *not* `Binary` or a UTF-16 variant. The binary NUL check only ever fires on input that has
    **already failed** UTF-8 validation for some other reason (e.g. a stray `0xFF`/`0x80`+ byte
    breaking a multi-byte sequence). Documented directly on `detect_encoding`.
  - **No-BOM UTF-16 is out of scope**, per the ticket: UTF-16 is detected via BOM only (`FF FE` /
    `FE FF`); no alternating-NUL-byte heuristic for BOM-less UTF-16 was added. Documented as a known
    limitation on `detect_encoding`.
  - **`LineEnding::Mixed` is unused by `detect_line_endings`:** the ticket's enum includes a `Mixed`
    variant, but also separately defines `dominant` as "the most common" of crlf/lf/cr with
    deterministic tie-breaking — which never naturally produces `Mixed`. Kept the variant in the
    public enum (as specified) but `dominant` never returns it; inconsistency (if a file is mixed)
    is conveyed via the separate `mixed: bool` field instead. Documented on the `LineEnding` type so
    this isn't mistaken for an oversight.
  - **Dominant tie-break order:** `Crlf` > `Lf` > `Cr` when counts tie — arbitrary but deterministic
    and stable, per the ticket's "ties handled deterministically" instruction. Covered by
    `dominant_tie_break_prefers_crlf_over_lf_over_cr`.
  - Scope note: epic CPE-1002 doesn't yet have a filed `Tickets/Epics/CPE-1002*.md` brief in this
    repo at the time of this ticket (same situation CPE-1001 hit for CPE-1000). Per the work order
    this ticket only touches `text_encoding.rs` + the one `lib.rs` module line + this ticket file, so
    no epic file was created here. Frontmatter still references `epic: CPE-1002` as instructed.
  - Verified (PowerShell, `crates/server`, `$env:PATH += ";$env:USERPROFILE\.cargo\bin"`):
    `cargo test --lib text_encoding` → 19/19 passed; `cargo clippy --all-targets -- -D warnings`
    clean; `cargo clippy --all-targets --features index -- -D warnings` clean. No clippy fixes were
    needed.
  - Status → Done; ACs checked; moving to
    `Tickets/Done/2026/Q3/July/Week-30/CPE-1003_text-encoding-detector.md`.
