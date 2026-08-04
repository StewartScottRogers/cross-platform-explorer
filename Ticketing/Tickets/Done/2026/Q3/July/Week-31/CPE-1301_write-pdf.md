---
id: CPE-1301
title: "write_pdf: incremental /Info metadata writer"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-725
---

## Summary
`read_pdf` reads the PDF `/Info` dictionary (Title/Author/Subject/Keywords) but there is no writer. Add
`write_pdf` via the standard, spec-sanctioned **incremental update**: append a new `/Info` object + a fresh
xref subsection + a new trailer to the END of the original bytes, leaving all original bytes untouched. This
is the safe approach — no rewriting of existing objects/offsets. Headless, cargo round-trip-tested.

## Build
- New `pub fn write_pdf(orig: &[u8], fields: &[MetaField]) -> Result<Vec<u8>, String>` in
  `crates/server/src/media_meta_write.rs`:
  - Find the original startxref / the highest object number and the previous xref offset from the trailer.
  - Emit a new indirect object for the `/Info` dict carrying the edited fields (escape PDF string syntax —
    `(...)` with `\`-escaping of `(`,`)`,`\`; or hex `<...>` — and handle the fields `read_pdf` exposes).
  - Append an incremental **xref** subsection referencing the new Info object (byte offset within the new
    combined buffer), and a new **trailer** with `/Root` carried from the original, `/Info <new obj> 0 R`,
    `/Prev <original startxref>`, and a `startxref` pointing at the new xref. Support both classic xref
    tables and, if the original uses an xref STREAM, degrade gracefully (either write a compatible xref
    stream, or return an honest `Err("xref-stream PDFs not yet supported")` rather than producing a broken
    file — pick one and document it).
  - Wire `"pdf" =>` into `media_meta::write_back` + add `pdf` to `is_writable`.
- No new dependency (the repo already parses PDF in `read_pdf`/thumb_pdf — reuse its lexing helpers if
  useful). NEVER panic — malformed/unsupported → `Err`.

## Acceptance criteria
- Round-trip: `read_pdf(write_pdf(orig, edits))` returns the edited Title/Author/etc.; the ORIGINAL bytes are
  a byte-for-byte PREFIX of the output (incremental-append property); the appended xref offset is correct so
  a compliant reader resolves the new `/Info`; `is_writable("pdf")` true; a malformed or xref-stream PDF
  returns a clear `Err`, never a broken file or panic.
- `cargo test -p cpe-server` green (round-trip on a crafted minimal classic-xref PDF + the prefix-property
  assertion + an Err case); `cargo clippy` clean both feature modes; no new dep; existing write tests pass.

## Notes
Hardest of the media codecs (xref/trailer arithmetic) — opus, careful review. Epic CPE-725. Shares
`media_meta_write.rs` + `media_meta.rs` with the merged write codecs — build on current main.

## Work Log
- 2026-08-03 — write_pdf merged (#598, ff-landed after clean auto-merge with the CPE-1300 audit test in media_meta_read.rs). Reviewer (opus) APPROVE: startxref offset exact (20-byte xref entry validated), round-trip works for escaped-parens + non-ASCII UTF-16BE, xref-stream refused honestly, editable-fields change required + assertions STRENGTHENED, panic-safe. 124 media tests + fixtures + clippy clean.
