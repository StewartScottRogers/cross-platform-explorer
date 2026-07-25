---
id: CPE-724
title: "EPIC: Code intelligence preview"
type: Task
status: In Progress
priority: Low
component: Frontend
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Upgrade the in-place text/code preview into a code-aware reading surface: a scrollable minimap, a symbol
outline / breadcrumb (functions, classes, headings) with jump-to-symbol, code folding, bracket/indent
guides, and an optional git-blame gutter.

## Why
The app already syntax-highlights ~200 languages; making large source files navigable (not just coloured)
is a natural, high-value extension for the developer audience that runs coding agents here.

## Rough scope (areas, not child tickets)
- Lightweight symbol extraction (tree-sitter-style) for the top languages.
- Minimap + symbol outline/breadcrumb with jump-to-symbol.
- Code folding and bracket/indent guides layered on the existing highlighter.
- Optional git-blame gutter for files in a repo.

## Open questions (resolve at activation)
- Symbol-extraction approach and per-language coverage vs. bundle size.
- Whether to adopt a code-editor component or extend the current renderer.
- Blame gutter cost and how it interacts with the in-place editor.

## Definition of Done
- Large source files show a minimap and a symbol outline with working jump-to-symbol.
- Folding and indent guides work across the top languages; blame gutter is optional.
- No regression to the existing preview/edit for plain text; extra features are lazy.

## Work Log
2026-07-22 (nightshift) — **Activated.** Open questions resolved (best-guess): symbol extraction is a
**dependency-free heuristic line scanner** (no tree-sitter → no native grammars / C build / bundle cost),
covering the top languages; the renderer is **extended** (not swapped for an editor component); blame is a
lazy, optional later child. First slice shipped: **CPE-910** — `cpe_server::code_outline` symbol
extraction for Rust / JS-TS / Python / Go / Markdown. Remaining children are GUI: minimap, outline/
breadcrumb with jump-to-symbol, code folding + indent guides, optional git-blame gutter.
