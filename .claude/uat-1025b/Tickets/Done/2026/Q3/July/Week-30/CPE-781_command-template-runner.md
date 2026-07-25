---
id: CPE-781
title: Pure command-template runner ({path}/{name}/{dir}/{ext}/{stem})
type: feature
status: Done
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed: 2026-07-20
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
- [x] Each token expands from the entry; unknown tokens untouched; `{{`/`}}` escape to literal braces.
- [x] `expandForSelection` produces one string per entry in "each" mode; empty selection → [].
- [x] Pure + dependency-free; unit tests cover tokens/escaping/edge cases; check + suite green.

## Notes
Independent of CPE-780. Consumed by CPE-783 (bound to toolbar/context-menu, run via a confirmed backend
exec). Headless.

## Resolution
Added `src/lib/cmdTemplate.ts` (pure): `expandTemplate(tpl, entry)` substitutes `{path}/{name}/{dir}/{ext}/
{stem}` (dir handles / and \; ext/stem handle extension-less names), leaves unknown `{tokens}` verbatim,
and unescapes `{{`/`}}`. `expandForSelection(tpl, entries, mode)` → one string per entry ("each") or a
single string with each token as a quoted space-joined list ("joined"); empty selection → []. Shell escaping
is deferred to the backend (CPE-783). 7 tests. check 0/0. Headless; no existing code touched.

