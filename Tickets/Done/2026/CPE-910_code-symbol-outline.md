---
id: CPE-910
title: Lightweight source-symbol outline (jump-to-symbol foundation)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-724
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
First headless slice of the code-intelligence preview (CPE-724): a **dependency-free** source-symbol
outline. `cpe_server::code_outline::outline(source, lang) -> Vec<Symbol>` returns functions / types /
classes / headings, each with a 1-based line, for a jump-to-symbol outline + breadcrumb over the existing
highlighter.

Open question resolved (best-guess): **heuristic line scanner, not tree-sitter** — no native grammars, no
C build, no bundle cost; per-language patterns for the top languages developers browse here. Covers:

- **Rust** — fn / struct / enum / trait / impl / mod / type / const / static (visibility + async/unsafe/const stripped).
- **JS/TS** — function / class / interface / enum / arrow-or-`function` assigned `const`/`let` (export/modifier stripped).
- **Python** — def / class, with indentation → method.
- **Go** — func (incl. method receivers) / type.
- **Markdown** — `#`..`######` headings by level.

## Acceptance Criteria
- [x] `outline` returns `{name, kind, line}` symbols for Rust / JS-TS / Python / Go / Markdown.
- [x] An unknown language → empty (never an error); mid-line `##` isn't a heading; a plain constant isn't a callable.
- [x] 6 tests (one per language + unknown); clippy `-D warnings` clean; no new deps; 3-OS.

## Work Log
- 2026-07-22 — Activated CPE-724 and shipped the symbol-extraction foundation. The minimap, the
  outline/breadcrumb UI with jump-to-symbol, folding, and the blame gutter are the GUI children.
