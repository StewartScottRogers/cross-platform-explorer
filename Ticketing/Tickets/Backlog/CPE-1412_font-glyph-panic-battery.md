---
id: CPE-1412
title: "Security: adversarial panic battery for font glyph rasterization (thumb_font.rs → ab_glyph SFNT/glyf)"
type: Task
status: Backlog
priority: Medium
component: Backend
tags: [ready]
epic: CPE-718
created: 2026-08-07
---

## Problem (untrusted-parser scout)
`crates/server/src/thumb_font.rs:render_glyph_sheet` parses opened `.ttf`/`.otf`/`.woff` file contents via
`ab_glyph`/`owned_ttf_parser` (SFNT/glyf/cmap tables). `unwrap_woff`'s own header math is hand-tested, but the
real SFNT/glyf walk inside `ab_glyph` has NO adversarial battery. TTF/OTF composite-glyph tables are a
historically panic-prone construct (same class the harness already flags for goblin/midly).

## Fix direction
Extend `crates/server/tests/binary_data_preview_panic_safety.rs` (`run_battery`, per the `pe_info`/`midi_info`
pattern) feeding `render_glyph_sheet` (or its path entrypoint) the realistic `DEMO_TTF` magic + fuzzed variants:
truncated tables, malformed composite-glyph recursion, bad cmap offsets, huge glyph counts, garbage. Assert
never panics. Report any real panic. `cargo test` + `cargo clippy --all-targets -- -D warnings` clean (Defender
note applies).
