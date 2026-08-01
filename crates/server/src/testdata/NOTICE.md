# Test fixtures

`demo.ttf` — a minimal, valid, 400-byte TrueType font used only by `thumb_font.rs`'s tests
(CPE-1236). Copied verbatim from the [`ttf-parser`](https://github.com/RazrFalcon/ttf-parser) crate's
own test suite (`tests/fonts/demo.ttf`), where it is also used as the crate's doc-test fixture for
`Face::outline_glyph`. MIT-licensed by its author, Yevhenii Reizner — same license as `ttf-parser`
itself (see that crate's `LICENSE-MIT`). It maps a single character, `'A'`, to glyph 1 with a real
outline; every other codepoint (including `'a'`) is intentionally unmapped, which is also useful for
exercising the "glyph not covered by this font" path.
