---
id: CPE-1413
title: "Security: adversarial panic battery for SVG rendering (thumb_svg.rs → resvg/usvg), incl. recursion"
type: Task
status: Backlog
priority: Medium
component: Backend
tags: [ready]
epic: CPE-718
created: 2026-08-07
---

## Problem (untrusted-parser scout — same shape as the WebDAV DoS)
`crates/server/src/thumb_svg.rs` renders opened `.svg` file contents via `resvg`/`usvg::Tree::from_data`. This is
an XML tree consumer — the exact shape of the WebDAV stack-overflow DoS (CPE-1398), just a different parser.
`<use>`/`<symbol>` self-reference and nested `<svg>` are recursion risks; usvg's 1M-element cap doesn't obviously
bound reference-cycle recursion. No adversarial coverage.

## Fix direction
Extend `crates/server/tests/binary_data_preview_panic_safety.rs` (or the appropriate harness) with SVG cases:
minimal valid `<svg>` + fuzzed tail, DEEPLY-NESTED `<g>`/`<svg>` (stack-overflow probe — test on a small-stack
thread like the webdav re-review did, since a stack overflow is uncatchable), a `<use xlink:href="#self">`
self-reference / reference-cycle case, huge element counts, garbage. Assert the render path returns Ok/Err and
NEVER crashes the process. If deep nesting or a reference cycle crashes (stack overflow) or hangs, STOP and REPORT
it as a real DoS (like CPE-1398) — a depth/size guard fix mirrors the webdav approach. `cargo test` + clippy clean.
