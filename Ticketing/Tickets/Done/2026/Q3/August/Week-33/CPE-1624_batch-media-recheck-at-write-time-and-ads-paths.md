---
id: CPE-1624
title: "Batch Media checks \"is this in place?\" once before the whole batch, never at each write — plus alternate-data-stream paths aren't recognised as the same file"
type: Bug
status: Backlog
priority: Medium
component: Backend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
The two lower-ranked findings from the independent Security Audit of CPE-1613 (PR #818). Neither is a
demonstrated single-actor data loss like CPE-1623, but both are real fail-open directions in a guard whose
entire job is preventing irreversible overwrites. Filed together because both are fixed in the same code.

## Finding A — TOCTOU: the guard is evaluated once for the batch, not per write
`execute_plan_walk` (`crates/server/src/batch_execute.rs:68-88`) calls `any_in_place(items)` **once**,
before the loop, then writes every item sequentially through `execute_one` with **no per-item re-check**.

Measured: `same_file(link.jpg -> A, A)` returns `true`; after swapping the symlink's target to `B`, a
repeat check correctly returns `false` — i.e. a check performed before a swap is stale for what actually
gets written afterwards, and nothing re-validates at write time. The window between the up-front check and
a given item's write grows with batch size and with slow per-file operations (watermark, compress).

Requires a local actor able to modify symlinks/junctions in the target tree concurrently with the batch —
another local process or a file-sync client, not a remote attacker. This is defence-in-depth rather than
demonstrated loss, but the cost of closing it is low.

**Fix:** re-check immediately before each write, not once for the batch — and on a late-detected in-place
collision, skip that item with a reported reason rather than writing it.

## Finding B — alternate data streams aren't recognised as the same file
Measured on a real file: `same_file("...\IMG_1.JPG", "...\IMG_1.JPG:hidden")` returns `false`.
`parent_and_name` (`batch_media.rs:463-479`, branches 2 and 3) splits on `/` and `\` only, so the colon
stays inside the name component and never matches lexically or under case-folding.

Writing to `file.jpg:hidden` does not clobber the primary `$DATA` stream a user sees, so no visible photo
is destroyed — but it writes to the same MFT record without the guard recognising the two paths as
related. It is **not reachable through `plan()`'s own path construction** (which never emits a colon);
it matters only if a caller supplies an ADS-style path directly (devtools or a future automation surface).

**Fix:** treat a Windows ADS suffix as part of the same underlying file for `same_file` purposes — or
reject ADS-style paths outright at the engine boundary, which may be the better answer given no legitimate
Batch Media flow produces one.

### Update 2026-08-11 — this is WIDER than first described, and it IS reachable from the UI
A second Security Audit (of CPE-1623, PR #828) **demonstrated** it rather than reasoning about it, and the
real shape is broader than "a caller supplies an ADS-style path directly":

`template_escapes_directory` (`batch_media.rs:145-147`) checks only `/`, `\` and `..`. **A colon anywhere in
a rename template passes `validate()` untouched**, and because the output is built by plain string
concatenation (`join()`, `batch_media.rs:198-204`) rather than `Path::join`, the colon survives into the
final path string — where NTFS interprets it as an alternate-data-stream separator.

Reproduced: template `"C:foo"` against an input in a folder that also contains a plain file named `C`.
`plan()` computed `...\workdir\C:foo.png` — **contained**, same directory, so the CPE-1623 containment check
correctly saw no escape — and `execute_plan` returned `Ok(written:1)`. The visible file `C` was unchanged in
size and content, while 120 bytes of transformed PNG were readable at `...\workdir\C:foo.png`, i.e. the
`foo.png` stream of file `C`. No confirmation was required, because `Path::is_file()` on a never-before-existing
stream path returns `false`.

So: not a directory escape (the data stays inside the chosen folder), but a **demonstrated hidden-data write
onto an unrelated, non-selected file**, invisible in Explorer and in the file's size — and reachable by an
ordinary user typing into the rename box, not just by a devtools caller. Any colon does it (`secrets:hidden`
works as well as a drive-letter shape), so scope the fix to **colons generally**, not to drive letters.

Also fold in from the same audit: the auditor's **Unicode slash look-alike** result (U+2215, U+FF0F, U+FF3C)
was **inconclusive** — they were rejected, which an ASCII-only `contains` check should not do, and it was not
root-caused. CPE-1623's follow-up fix is being asked to answer that definitively; if it does not, settle it
here rather than leaving it unknown.

## Acceptance criteria
- A test demonstrates the per-write re-check catches a collision introduced *after* the initial batch check
  (simulate by swapping a symlink/junction target mid-batch), and that the affected item is skipped with a
  reported reason rather than written.
- `same_file` (or the engine boundary) handles the ADS case; a test covers `X.JPG` vs `X.JPG:stream`.
- No regression in ordinary batches: no new false alarms, and the common-case cost stays negligible —
  measure and state the added per-item overhead.
- Tests must not assert exact filesystem byte counts (CI runs Linux + macOS + Windows).

**Conflict surface:** `crates/server/src/batch_execute.rs`, `crates/server/src/batch_media.rs` (`same_file`,
`parent_and_name`), plus their tests. Overlaps CPE-1623 — **work them in sequence, not in parallel.**

## Notes
Findings the same audit tried and could NOT break (recorded so nobody re-treads them): trailing dot/space,
8.3 short names, `\\?\` extended-length prefixes, junctions and NTFS symlinks, `CON` device names, Turkish
dotless/dotted I, NFC vs NFD on Windows, and `.`/`..` handling inside `same_file` itself. The Kelvin-sign
case over-matches (says "same" where NTFS says "different") — the safe direction, so it is not a defect.
