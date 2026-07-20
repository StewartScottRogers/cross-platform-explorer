---
id: CPE-781
title: Pure command-template runner ({path}/{name}/{dir}/{ext}/{stem})
type: feature
status: Open
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed:
epic: CPE-711
estimate: 1h
---

## Summary
Foundation for user-defined commands (epic CPE-711). A pure module (`src/lib/cmdTemplate.ts`) that expands a
command template against a file entry (and a selection) — so the command-binding UI (CPE-783) is a thin
render + a backend exec call.

## Scope
- `expandTemplate(tpl: string, entry: DirEntry): string` substituting `{path}` `{name}` `{dir}` `{ext}`
  `{stem}` (name without extension). Unknown `{...}` tokens are left verbatim; `{{`/`}}` escape braces.
- `expandForSelection(tpl, entries, mode: "each" | "joined"): string[]` — one expansion per entry, or a
  single expansion where a list token joins the selection (per-item is the default).
- Pure + total (no tokens, repeated tokens, empty selection, extension-less names).

## Acceptance Criteria
- [ ] Each token expands from the entry; unknown tokens untouched; `{{`/`}}` escape to literal braces.
- [ ] `expandForSelection` produces one string per entry in "each" mode; empty selection → [].
- [ ] Pure + dependency-free; unit tests cover tokens/escaping/edge cases; check + suite green.

## Notes
Independent of CPE-780. Consumed by CPE-783 (bound to toolbar/context-menu, run via a confirmed backend
exec). Headless.
