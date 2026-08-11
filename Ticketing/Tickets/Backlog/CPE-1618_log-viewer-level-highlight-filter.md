---
id: CPE-1618
title: "Log viewer — per-line level detection with colour highlight + filter"
type: Feature
status: Backlog
priority: Medium
component: Frontend
epic: CPE-1568
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Epic CPE-1568 slice 8 ("stretch"). Verified against real code: `.log` files map to the `accesslog`
highlight.js grammar in `src/lib/preview/highlight.ts` (`log: "accesslog"`) and otherwise render through
the generic `text` provider — plain syntax-colored text, no level-aware structure. No log-specific provider
exists in `src/lib/preview/provider.ts`. This is high-value for the app's own audience: `.log` files are
exactly what a developer/agent-operator opens most when debugging, and the app already leans into
developer workflows (Workbench, Agent Watch, code-intel preview). Pure frontend, no backend involvement.

## Goal
Selecting a `.log` file (or any file whose lines look like leveled log output) shows each line's severity
level highlighted, with a filter control to show only WARN/ERROR (or a chosen level and above).

## Scope
**Conflict surface:** new `src/lib/preview/logViewer.ts` (+ `.test.ts`) and new
`src/lib/components/LogPreview.svelte` (+ `.test.ts`); one new provider entry in
`src/lib/preview/provider.ts`; one new else-if branch in `src/lib/components/PreviewPane.svelte`.
**Shares `provider.ts` and `PreviewPane.svelte` with CPE-1616 and CPE-1617 — see CPE-1616's note on
serializing those two files' merges.** No backend/`cpe-server` changes, no new dependency (this is pure
regex/string parsing, unlike CPE-1617's YAML/TOML parser need).

- `logViewer.ts`: a pure `detectLevel(line: string): LogLevel | null` function recognizing common leveled
  formats by regex — `ERROR`/`ERR`, `WARN`/`WARNING`, `INFO`, `DEBUG`/`DBG`, `TRACE` (case-insensitive,
  matched as a whole word near the line start or after a timestamp/bracket, the shape most log formats
  share: `[2026-08-11 12:00:00] ERROR ...`, `ERROR: ...`, `E/tag: ...` Android-style, etc. — cover the
  common cases, don't try to be a universal log-format parser). Lines matching nothing render as
  plain/unleveled (never misclassified as an error by a greedy regex).
- `LogPreview.svelte`: per-line rows (reuse the per-line rendering approach `hljs-blob-to-per-line-rows`
  established for code preview — Library entry `hljs-blob-to-per-line-rows.md` — for the gutter/row
  structure precedent), each row tinted by detected level using existing theme tokens (`var(--danger)` for
  error, a warn tone, etc. — never a hard-coded hex, per the theme-token convention already enforced
  elsewhere in this codebase). A **level filter** control (checkboxes or a dropdown: All / Warn+ / Error
  only) hides non-matching rows client-side — no re-fetch, just a filtered render.
- Register the `log` provider: `canPreview: (e) => !e.is_dir && e.extension === "log"`, declared before the
  generic `text` provider.
- Cap rendering for a huge log file (streaming/paged, not one giant unvirtualized list) — reuse the
  existing capped-render precedent (`capRows`-style, "showing first N of M lines" note) rather than trying
  to virtualize a DOM list from scratch; a multi-hundred-MB log is out of scope (respect the existing
  preview size ceiling other providers already enforce).

## Explicitly NOT in scope
- No log tailing / live-follow of a growing file — this is a static preview like every other provider, not
  a `tail -f`.
- No structured/JSON-log (e.g. one-JSON-object-per-line) special-casing in v1 — plain leveled text lines
  only; note it as a follow-up if it comes up, don't scope-creep this ticket to cover it.
- No full log-format auto-detection beyond the common regex shapes listed above — a log with an unusual
  format degrades gracefully to unleveled plain text, not a crash or a wrong classification.

## Acceptance criteria
- A `.log` file with a mix of INFO/WARN/ERROR lines renders each line tinted by its detected level.
- The filter control actually hides non-matching lines and the count updates.
- A log with no recognizable level markers renders as plain text, not misclassified.
- A very large log file stays responsive (capped/paged rendering, honest "showing N of M" note).
- `npm run check` and the new Vitest suites green.

## Notes
Model: sonnet. Library entry: `filetype-right-pane-coverage-2026-08-10` (epic spike),
`hljs-blob-to-per-line-rows.md` (per-line row/gutter precedent).
