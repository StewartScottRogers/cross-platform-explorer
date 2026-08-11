---
id: CPE-1618
title: "Log viewer — per-line level detection with colour highlight + filter"
type: Feature
status: Doing
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

## Work Log

### 2026-08-11 — implemented, PR opened

Built the log viewer exactly to Scope, mirroring CPE-1616's notebook-viewer shape (pure parser module +
self-contained Svelte component + one additive provider entry):

- `src/lib/preview/logViewer.ts`: pure, framework-free `detectLevel`/`parseLog`/`filterLines`. Recognizes
  ERROR/ERR, WARN/WARNING, INFO, DEBUG/DBG, TRACE (case-insensitive) via bracketed/ISO-timestamp prefixes,
  `LEVEL:`/`[LEVEL]` shapes, and Android logcat's `E/Tag:` single-letter style. Deliberately conservative:
  a level-word match is only accepted when its lead-in (the text before it, within an 48-char scan window)
  contains no lowercase letter — this is what stops a line that merely *mentions* "error" in prose
  ("User asked about a checkout error they saw yesterday") from being misclassified, per the ticket's
  explicit acceptance criterion. Reused `stripAnsi` from `notebook.ts` rather than duplicating the regex.
- `src/lib/components/LogPreview.svelte`: per-line rows (gutter + level badge + text) in a bounded,
  scrollable region (`.log-body`, `overflow-y:auto`, mirrors `HexView.svelte`'s `height:100%` flex-fill
  convention — the outer `<aside class="preview">` is the app's real scroll container, so a self-contained
  provider fills 100% of it and scrolls internally). Level filter is a row of reflowing chips (one per
  level + "Other" for unleveled lines), each showing a live count; clicking toggles it and the visible
  rows/count update instantly, client-side, no re-fetch.
- `provider.ts`: new `"log"` `PreviewKind` + one provider entry (`canPreview: e.extension === "log"`),
  placed before `markdown`/`text` since `.log` categorises as `"text"` in filetypes.ts.
- `PreviewPane.svelte`: one import line + one `{:else if provider.kind === "log" && entry}` branch,
  positioned right after the `notebook` branch — same minimal-additive shape CPE-1616 used, to keep the
  merge with sibling ticket CPE-1617 (YAML/TOML) clean.

**Caps bound WORK, not just output** (the crew's CPE-1616 font-cache lesson): `PREVIEW_MAX_BYTES` (256 KiB,
reused from `loaders.ts` — no new read cap invented) already bounds the file read regardless of on-disk
size, so a 500 MB / 10-million-line file never gets past that. Within the capped text, `MAX_LINES` (5000)
is sliced off the split array BEFORE any per-line work (detect/strip/cap), and `detectLevel` only ever
inspects the first 48 characters of a line — so neither a huge line count nor one pathological huge single
line (up to the 256 KiB read cap, no newlines) can make detection do unbounded work.

**Adversarial inputs tested** (`logViewer.test.ts`, 36 cases; `LogPreview.test.ts`, 13 cases):
- A single 50,000-char line — capped to `MAX_LINE_CHARS` (2000) for render, `truncated` flagged, level
  detection still correct (only scans the head).
- `MAX_LINES + 1000` lines mixed with periodic 20,000-char ANSI-noise lines — completes in well under the
  test's 2s bound (not a hang/freeze).
- Prose lines that mention "error"/"warning" mid-sentence — confirmed NOT misclassified (both in the pure
  parser and end-to-end through the mounted component).
- `ERRORLEVEL=1` (word-boundary false-positive trap), a lowercase `e/notactuallyalevel:` (Android-style
  false-positive trap), empty string, and a 1,000,000-char line — all handled without throwing.
- Real ANSI SGR escape bytes (`ESC[31mERROR ESC[0m …`, built from the actual escape byte, not literal
  `"[31m"` text) — stripped cleanly before render and before/during detection; verified end-to-end that no
  literal escape-code garbage reaches the DOM.
- A malformed/failed load (`readFileText` rejecting) renders a distinct `log-load-error` state, never an
  empty-looking pane; an empty-but-valid file renders a distinct "This file is empty." note — the two are
  structurally different DOM states (CPE-1616 lesson: never render "couldn't read this" as "nothing here").

**Theme tokens**: one new scoped token, `--log-warn` (amber), added to the palette + all three of bare
`:root`, `:root[data-theme="light"]`, `:root[data-theme="dark"]` in `app.css` — computed contrast (script,
not guessed): light `#8a5a00` = 5.93:1 on white / 5.34:1 on `--pal-gray-100`; dark `#ffb84d` = 8.24:1 on
`--surface` / 9.48:1 on `--bg` — both clear WCAG AA. ERROR/INFO reuse the existing `--danger`/`--accent`
tokens rather than adding new ones. Did NOT touch `--danger` or any other shared token (learned from the
sibling ticket that regressed destructive-button contrast app-wide by nudging a shared token — CPE-1632
tracks that pre-existing issue separately, not touched here). Caught my own near-miss before it shipped:
an initial `.log-chip.active` used `color: #fff` on a solid `var(--accent)` fill — the exact
white-on-solid-colour pattern CPE-1632 flags as already under-contrast elsewhere — so I reworked active
chips to a `color-mix` tint + coloured text/border instead (no new token needed, and it also kept the
component hard-coded-hex ratchet test at its 90-file baseline instead of growing it to 91).

**Sample fixture**: `samples/text/app.log` (new, added via `scripts/gen_samples.py`'s `TEXT_FILES` dict +
a targeted `write()` call — did not re-run the full `main()`, which would have touched every
ffmpeg/PIL-encoded binary fixture unnecessarily). Exercises every recognized shape, the ANSI-wrapped line,
and the deliberate "mentions error but isn't leveled" prose line. Satisfies `sampleCoverage.test.ts`'s
ratchet for the new `"log"` kind. Also added a row to `samples/README.md`'s fixture table + its
kind→sample mapping table (the latter was already missing a few older kinds like jwt/cert — left those
as pre-existing gaps, out of scope here).

**Docs**: followed the CPE-1616 precedent exactly — a preview *provider* is not a `Section` in
`sectionDocs.ts`'s sense (that enum is app-level navigation surfaces: Explorer, Agent Watch, Settings,
…), and the notebook viewer didn't add a docs entry either, so this doesn't either. No `sectionDocs.test.ts`
change needed.

**Verification** (all commands run synchronously in this worktree):
- `npm run check` → `svelte-check found 0 errors and 0 warnings`.
- `npx vitest run` (full suite) → **`Test Files 277 passed (277)`, `Tests 3450 passed (3450)`**. No
  existing test touched or weakened.
- Targeted: `logViewer.test.ts` 36/36, `LogPreview.test.ts` 13/13, `provider.test.ts` 44/44,
  `sampleCoverage.test.ts` 4/4, `app.css.test.ts`/`app.css.dark-contrast.test.ts`/`app.css.hc-contrast.test.ts`
  38/38 (all green after the new `--log-warn` token).

**Shared files touched** (CPE-1617 is held pending this landing): `src/lib/preview/provider.ts` (+1
`PreviewKind` union member, +1 provider object) and `src/lib/components/PreviewPane.svelte` (+1 import
line, +1 `{:else if}` branch) — both minimal additive edits, matching CPE-1616's shape exactly.

**Not independently verified**: real-browser visual appearance (chip reflow, row tint, scroll bounding) —
jsdom asserts DOM/CSS-rule presence only, per the structural-guard convention; needs a Visual Critic
screenshot pass like every other layout claim in this codebase.

Pushed to `cpe-1618-log-viewer`; PR opened. Ticket left in `Doing/`.
