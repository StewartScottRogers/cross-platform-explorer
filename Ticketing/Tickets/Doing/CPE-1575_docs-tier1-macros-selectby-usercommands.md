---
id: CPE-1575
title: "Docs Tier-1 batch B: Macros + Select By… + User-defined commands pages"
type: Task
status: Doing
priority: High
component: Frontend
epic: CPE-1569
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1569 slice 4 — the remaining Tier-1 MISSING doc pages (shipped features with zero docs, per the audit).

## Scope — three new `src/docs/*.md` pages, each to the 10-point rubric, verified against the components
1. **Macros / scriptable actions** — `src/lib/components/MacrosDialog.svelte` (+ `MacroRunConfirm.svelte`,
   `MacroParamPrompt.svelte`, `src/lib/macroBindings.ts`): recording/defining macros, parameter prompts, hotkey/menu
   binding, run-confirm. May warrant 2 pages (author/bind vs run) — use judgment. Category: Organizing & Tagging.
2. **Select By…** — `src/lib/components/SelectByDialog.svelte` (and/or `PatternSelectDialog.svelte`): criteria
   (pattern/type/date/size), invert, same-extension. Category: Organizing & Tagging.
3. **User-defined commands** — `src/lib/components/UserCommandsDialog.svelte` (+ `RunCommandConfirm.svelte`):
   template commands with `{path}/{name}/{dir}/{ext}/{stem}`, confirm-before-shell. Category: Organizing & Tagging.

Rubric per page: what-it-is · when-to-use · how-to-open (menu+palette+shortcut) · every option · every action+shortcut
· ≥1 worked example · cross-links · required "Limits/notes" section · safety framing (esp. User-commands = runs shell!)
· accuracy verified against the component. Category-prefixed slugs; add to `src/docs/00-index.md`.

## Acceptance criteria
- New pages exist, categorized, in `DocsView` + index; cross-links resolve; accurate to components; no padding.
- **Shrink `docs.coverage.test.ts` allowlist**: remove `MacrosDialog`, `SelectByDialog`, `UserCommandsDialog`
  (now documented). If `PatternSelectDialog`/`MacroRunConfirm`/`MacroParamPrompt`/`RunCommandConfirm` are covered by
  the new pages' text, remove those too; otherwise leave with an updated reason.
- `sectionDocs.test.ts` green; `npm run check` clean; vitest green (incl. coverage guard).

## Notes
Docs-only + guard allowlist edit — disjoint from backend/preview work. Base off current main. See Library
`docs-completeness-audit-2026-08-10`. Model: sonnet.
