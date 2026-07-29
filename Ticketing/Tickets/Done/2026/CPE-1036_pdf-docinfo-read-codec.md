---
id: CPE-1036
title: PDF document-info read codec (Title/Author/… → MetaFields)
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-25
epic: CPE-725
estimate: 2-3h
---

## Summary
Broadens the media-metadata studio's **read** coverage from audio/images to **documents** (epic
CPE-725). Adds a pure, headless PDF document-info reader that surfaces a PDF's standard `/Info`
dictionary — Title, Author, Subject, Keywords, Creator, Producer, CreationDate, ModDate — as
`MetaField`s in group `"pdf"`, matching the existing read-codec shape (`read_id3v2` / `read_exif`).
Also feeds the CPE-707 metadata columns.

Add to `crates/server/src/media_meta_read.rs`:
- `pub fn read_pdf(bytes: &[u8]) -> Vec<MetaField>` — return empty when there's no `%PDF-` header.
  Locate the trailer's `/Info N 0 R` reference (scan the last trailer dictionary; fall back to scanning
  for an `/Info` ref anywhere), resolve that indirect object, and extract its string entries. Decode
  both **literal** `( ... )` strings (with `\)` `\(` `\\` `\n` `\t` escapes + `\ddd` octal) and
  **hex** `< ... >` strings; map PDF keys to friendly ones (`/Title`→"Title", `/CreationDate`→"Date
  Created", etc.). All fields `group: "pdf"`, `editable: false` (write-back is a later ticket).
- Robust + never-panic: tolerate compressed/object-stream PDFs by simply returning whatever plain
  entries are found (empty is acceptable) — never scan unbounded, never index out of range.

## Acceptance Criteria
- [ ] `read_pdf(bytes)` on a minimal in-test-constructed PDF with an `/Info` dictionary returns the
      expected Title/Author/Subject/Producer fields (group `"pdf"`), decoding both a literal `(...)`
      string and a hex `<...>` string, including an escaped `\)` and an octal `\251`.
- [ ] Returns empty (not a panic) for: non-PDF bytes, a truncated/garbage PDF, and a PDF with no
      `/Info`. A lying/huge length can't over-read.
- [ ] Pure `std`, **no new deps**; matches the existing `MetaField` shape so `column_extract` can reuse
      it later.
- [ ] `cargo test -p cpe-server` green; `cargo clippy -p cpe-server --all-targets -D warnings` clean in
      **both** feature modes (default and `--features specta`).

## Work Log
2026-07-25 (workshift) — Filed + dispatched to a worker. Extends the read-codec arc
(ID3/FLAC/OGG/EXIF → +PDF documents). Disjoint file scope from CPE-1035 (which owns the new
`media_meta_write.rs`).

2026-07-25 (workshift) — **DONE, merged PR #354.** `media_meta_read::read_pdf` surfaces a PDF's `/Info`
dictionary (Title/Author/Subject/Keywords/Creator/Producer/Date Created/Date Modified) as
`MetaField{group:"pdf",editable:false}`; decodes literal/hex/escaped/octal/UTF-16BE strings; bounded,
never-panics. **QA gate caught + fixed a real bug:** first review (CHANGES REQUESTED) found the
object-resolver matched `5 0 obj` inside `15 0 obj` (wrong /Info object) — Foreman applied a token-boundary
fix + 2 regression tests; independent re-review APPROVED and proved the decoy test has teeth (fails without
the fix). UAT PASS. Clippy clean both modes. Read arc now: audio + image + video + documents.
