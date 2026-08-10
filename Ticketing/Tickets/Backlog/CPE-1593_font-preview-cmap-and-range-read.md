---
id: CPE-1593
title: "Font preview follow-ups: real cmap coverage, range-read metadata, and parser fuzz regression tests"
type: Task
status: Backlog
priority: Low
component: Frontend
epic: CPE-1568
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Follow-ups raised by the independent Reviewer and UAT Tester on CPE-1586 (font glyph grid, PR #798). None
blocked the merge; all three are real improvements.

## Items
1. **Glyph grid shows a fixed Latin sample, not the font's actual coverage.** The grid renders a fixed
   Basic-Latin + Latin-1 candidate list (189 codepoints), so a CJK font (`malgun.ttf`), a symbol font
   (`seguisym.ttf`), or a historic-script font shows only Latin glyphs — none of the characters that make
   that font distinctive. Parse the font's **cmap** table and drive the grid from real coverage (still
   capped/virtualized per STREAMING.md). Until then, the docs phrasing in
   `src/docs/30-structured-previews.md` ("eyeball its coverage") oversells for non-Latin fonts — soften it
   or fix it properly here.
2. **The font file is read twice and fully.** `FontPreview.svelte`'s `load()` lets `FontFace` fetch the file
   for rendering and then `fetch(...).arrayBuffer()`s the whole file again just to sniff format + the
   `name`/`maxp` tables — typically only the first few KB are needed. For a multi-MB CJK font that is double
   I/O and double memory, against PURPOSE.md's fast/small/predictable tiebreaker. Read only a leading byte
   range for metadata sniffing. (Note: this mirrors an existing pattern in `PreviewPane.svelte`'s
   `copyImageToClipboard`, so consider fixing both.)
3. **Lock the parser's safety into regression tests.** The Reviewer fuzzed `parseSfntMetadata` with 9 crafted
   malformed inputs plus 20 rounds of random bytes and it degraded gracefully every time — but the shipped
   suite doesn't encode that. Add at least: table offset past EOF, `ttcf` with a bogus sub-font offset, and a
   huge `name`-record count. Also: `parseNameTable`'s `length` parameter is never used, so a corrupt
   `count`/`stringOffset` could read strings from a neighbouring table (never out of bounds, so not a crash —
   a correctness nit). Either use `length` to bound reads to `[offset, offset+length)` or drop the parameter.

## Notes
Small, self-contained, no new dependencies. Conflict surface: `src/lib/preview/font.ts`,
`src/lib/components/FontPreview.svelte`, `src/lib/preview/font.test.ts`, `src/docs/30-structured-previews.md`.
Model: sonnet.
