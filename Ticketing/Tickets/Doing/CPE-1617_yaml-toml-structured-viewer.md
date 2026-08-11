---
id: CPE-1617
title: "YAML/TOML structured view + validate — not just ini/yaml syntax-highlighted text"
type: Feature
status: In Progress
priority: Medium
component: Frontend
epic: CPE-1568
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Epic CPE-1568 slice 7. Verified against the real code: `LANG_BY_EXT` in `src/lib/preview/highlight.ts`
maps `yaml`/`yml` to the `yaml` highlight.js grammar and — more tellingly — **`toml` to the `ini`
grammar** (an approximation, not real TOML parsing). Neither format has a dedicated provider in
`src/lib/preview/provider.ts`; both fall through to the generic `text` provider today, same as any plain
code file. There is no structured tree, no validate action, and no dependency for parsing either format
anywhere in the repo (`package.json` has no `js-yaml`/`yaml`/`toml`/`smol-toml`; `crates/server/Cargo.toml`
has no `serde_yaml`/`toml`). The existing `json` provider is the template to follow: it's a **pure-frontend**
provider (`needsText`) with a `JsonTree.svelte` structured view plus Format/Validate actions implemented
entirely client-side against `ctx.text` — no backend involvement at all. YAML/TOML can follow the exact
same shape.

## Goal
A `.yaml`/`.yml`/`.toml` file gets a structured (tree-style) view of its parsed content, plus a Validate
action that reports a real parse error instead of only generic syntax coloring.

## Scope
**Conflict surface:** new `src/lib/preview/yamlToml.ts` (+ `.test.ts`); reuse or lightly generalize
`src/lib/preview/jsonTree.ts`/`JsonTree.svelte` for the tree rendering (YAML/TOML parse to the same
plain-JS value shape JSON does — object/array/string/number/boolean/null — so the existing tree component
should need at most a prop-level generalization, not a fork); one new entry each for `yaml` and `toml` in
`src/lib/preview/provider.ts`'s provider array; one new else-if branch (or a shared one, since both parse
to the same tree shape) in `src/lib/components/PreviewPane.svelte`. **Shares `provider.ts` and
`PreviewPane.svelte` with CPE-1616 and CPE-1618 — see CPE-1616's note on serializing those two files'
merges.** Also touches `package.json`/`package-lock.json` (new parse dependency) but **not**
`crates/server/`, `src-tauri/`, or any Rust file — no backend work, so no overlap with CPE-1615 or any
in-flight backend ticket.

- Add one small, permissively-licensed, dependency-free-of-native-code JS parser per format (e.g. `yaml` or
  `js-yaml` for YAML; `smol-toml` or `@iarna/toml` for TOML — pick the smallest well-maintained MIT/ISC
  option; dynamic-`import()` it from the preview module only, like `highlight.ts` already dynamic-imports
  its language grammars, so it never bloats the base bundle for users who never open a YAML/TOML file —
  this is the PURPOSE.md fast/small/predictable tiebreaker in practice).
- `yamlToml.ts`: pure functions `parseYaml(text)`/`parseToml(text)` returning `{ok: true, value} | {ok:
  false, error}` (mirror `parseJson`'s shape in the existing json-provider code), plus a `format`
  passthrough if the chosen libraries support round-tripping (nice-to-have, not required).
- Register `yaml`/`toml` providers: `canPreview` on extension (`yaml`/`yml` and `toml` respectively),
  declared before the generic `text` provider (same ordering precedent as `json`/`csv`). Give each a tree
  view (reusing/generalizing `JsonTree.svelte`) plus **Format** and **Validate** actions matching the
  `json` provider's action shape (`copy-value`/`copy-path` optional if the tree component doesn't already
  support focus-tracking for these formats — Validate is the must-have, the rest is bonus parity with
  JSON).
- Handle files that fail to parse (invalid YAML/TOML) — Validate must show the real parser error message,
  and the tree view must degrade to the plain-text/syntax-highlighted fallback rather than a blank pane.

## Explicitly NOT in scope
- No YAML **anchors/aliases/merge-key** resolution beyond what the chosen library does automatically — this
  is a viewer, not a YAML processor; document the honest limit if the library doesn't fully resolve them.
- No editing-the-tree-and-writing-back — `editable: true` (raw text edit) can stay as-is, same as JSON's
  existing edit path; this ticket is the read/validate side only.

## Acceptance criteria
- A real multi-level `.yaml` and `.toml` file each render as a structured tree, not flat highlighted text.
- Validate reports a specific, real parser error on deliberately broken input for both formats (e.g. bad
  indentation for YAML, a malformed table header for TOML).
- `npm run check` and the new Vitest suites green.
- `Cargo.lock`/`crates/server/Cargo.toml` are untouched — confirms this stayed frontend-only as scoped.

## Notes
Model: sonnet. Library entry: `filetype-right-pane-coverage-2026-08-10` (epic spike).

## Work Log
2026-08-11 — Picked up, unblocked behind CPE-1616/CPE-1618 (both merged). Read both sibling viewers
(`notebook.ts`+`NotebookPreview.svelte`, `logViewer.ts`+`LogPreview.svelte`) as the template: pure
framework-free parser module + a self-contained Svelte component that fetches/parses its own content,
no provider-declared action-bar actions.
2026-08-11 — Dependency decision (per Foreman direction, overriding the ticket's suggested
smol-toml/js-yaml pick): hand-rolled both parsers instead of adding a dependency. TOML
(`src/lib/preview/toml.ts`): a real recursive-descent parser covering tables, dotted keys,
`[table]`/`[[array.of.tables]]` headers, inline tables, single- and multi-line arrays, basic/literal
strings, all numeric forms (dec/hex/oct/bin, inf/nan), with anything out of that tractable subset
(multi-line `"""`/`'''` strings) surfacing as a specific parse error rather than a silent guess. YAML
(`src/lib/preview/yaml.ts`): a BOUNDED SUBSET only — block mappings/sequences, the `- key: value`
shorthand, single-line flow collections, plain/quoted scalars. Anchors, aliases, tags, block scalars
(`|`/`>`), complex mapping keys, multi-document streams, tab indentation, and a flow collection
spanning multiple lines all degrade EXPLICITLY (`unsupported:true` + a specific reason), never
silently mis-parsed; a genuine syntax error is a structurally distinct `unsupported:false` result.
2026-08-11 — Reused `JsonTree.svelte`/`jsonTree.ts` UNCHANGED (no fork, no prop-level generalization
needed) by round-tripping the parsed value through `JSON.stringify` before handing it to `<JsonTree
text={...}>` — both parsers only ever produce the same plain-JS value shapes JSON does, so this is
lossless and gets `JsonTree`'s own `MAX_CHILDREN`/`AUTO_COLLAPSE_DEPTH` render-safety caps for free.
2026-08-11 — Built `YamlTomlPreview.svelte`: five structurally distinct states (loading / load-error /
empty file / parse error / deliberately-unsupported degrade / success tree), each its own
`data-testid`, per the "never conflate a parse failure with an empty file" lesson from CPE-1591/1615/
1618. Registered `yaml`+`toml` `PreviewKind`s and two provider entries in `provider.ts` (extension-gated,
`editable:false`, no `actions`), one new `{:else if}` branch in `PreviewPane.svelte` — additive-only,
mirroring notebook/log exactly to avoid the parallel-PR shared-file conflict this crew has hit before.
Skipped i18n for the new component (matches the Notebook/Log precedent — neither uses `$t()` either;
adding new keys would require translating them across all `COMPLETE_LOCALES`, a much larger diff for a
component pattern that already doesn't participate in i18n).
2026-08-11 — Added `text/config.toml` + `text/config.yaml` samples (`scripts/gen_samples.py` dict entries
+ the actual files) to satisfy `sampleCoverage.test.ts`'s ratchet — a first draft nested
`limits`/`allowed_ports`/`dependencies` after `[server.tls]`, which is CORRECT TOML semantics for
"these keys belong to server.tls", not what was intended; caught by writing a throwaway verification
test that printed the parsed JSON, fixed by moving those keys before the first table header, verified,
then deleted the throwaway test. Updated `samples/README.md`'s two tables. Added a one-`it()` capture to
`gui-smoke/specs/preview-pane.smoke.ts` (its own README documents this as a "one-line recipe" for a new
provider) — not run locally (needs a real `tauri build` + `tauri-driver`/WebDriver stack).
2026-08-11 — `npm run check`: 0 errors, 0 warnings. `npx vitest run`: 286 files, 3556 tests passed, 0
failed (60 new parser tests + 8 new component tests). No `crates/server/`, `src-tauri/`, or `Cargo.*`
file touched — confirmed via `git status`, satisfying the ticket's frontend-only acceptance criterion.
2026-08-11 — PR #833 review round 1 (CHANGES REQUESTED): five real inputs previously parsed WRONG
(accepted as valid) instead of erroring — the exact failure mode the ticket named as worse than not
supporting a format. Fixed all five, each with a negative-control test documenting the pre-fix failure:
(1) YAML `key: value: value` (a bare, unquoted mapping-colon inside a value) now rejected — every real
tool (js-yaml, PyYAML) rejects it too; (2) TOML leading-zero integers/floats (`007`, `03.14`) now
rejected per spec ("Leading zeros are not allowed"); (3) TOML re-opening an already-`[table]`'d name now
rejected ("tables cannot be defined more than once"); (4) a TOML `[table]` header colliding with a table
already established via a dotted-key assignment now rejected — the spec's own worked "DO NOT DO THIS"
example; (5) a TOML `[table]` header colliding with an established `[[array of tables]]` now rejected.
Findings 3-5 share one mechanism: `Parser`'s new `explicitTables`/`dottedKeyTables` WeakSets track HOW
each table node came into existence (explicit header / array-of-tables entry / dotted-key touch) so
illegal transitions are rejected without three bolted-on special cases — a legal "super-table defined
afterward" (`[x.y.z]` then `[x]`) stays legal, confirmed by its own test.
2026-08-11 — Same PR, a second UAT pass (20 real config files off-machine: TOML 7/7 correct, YAML 4/13
structured / 7/13 honestly degraded / 2/13 falsely called broken) surfaced the INVERSE failure mode —
valid YAML wrongly rejected or wrongly degraded. Fixed both, each with a negative-control test: (6)
indentless sequences (`status:\n- item\n- item` — spec-legal, PyYAML parses it fine) previously reported
"unexpected indentation"; now parsed via a same-indent `parseSequence` call from the mapping's own
`rest===""` branch. (7) a double-quoted scalar wrapping across physical lines (YAML line-folding, found
on a real 403-line locale catalog) previously reported "unterminated string"; `tokenizeLines` now folds
it during the raw physical-line pass (bounded to `MAX_QUOTE_FOLD_LINES=200` continuation lines as a
safety valve against a GENUINELY unterminated quote silently swallowing unrelated lines — falls back to
the pre-existing safe error path if the cap is hit). (8) Assessed block scalars (`|`/`>` + chomping) as
TRACTABLE — unlike anchors/aliases/tags/complex-keys, a block scalar's body is just "physical lines
indented more than the header, joined by one of two rules, then chomped", a linear scan this line-based
parser already does — and the single biggest real-world driver of YAML degrading to plain text (every
GitHub Actions `run: |` step). Implemented it: `tokenizeLines` detects a block-scalar header (bare `|`,
`>`, `|-`, `|+`, `>-`, `>+` — the rarer explicit-numeric-indentation-indicator form like `|2` stays an
honest "unsupported" degrade) and consumes the body directly off the RAW physical lines (blank lines
inside a block scalar are significant content, unlike between ordinary mapping/sequence entries, which
is why this can't run against the already-blank-filtered `Line[]` array). One real bug caught mid-
implementation and fixed before landing: the block-value check in `parseSequence` initially fired for
ANY dash line carrying a resolved block value, incorrectly stealing the `- key: |` (dash + inline key)
shape away from the nested-mapping parse that should own it — guarded with
`BLOCK_SCALAR_HEADER_RE.test(rest)` so only a BARE `- |` dash item claims it directly; `- key: |` now
correctly recurses into `parseMapping`, which attaches the block value to the `key`. Verified against a
3-level-deep synthetic GitHub Actions shape (`jobs.build.steps[0].run: |`) end to end.
2026-08-11 — Two trivial fixes from the same review: (9) the degrade/error banners read as run-on
sentences ("...this preview Showing the raw file..." with no punctuation) — added a period between the
interpolated reason and the follow-up sentence in both banners. (10) A whitespace/comment-only file
(non-zero bytes) parsed successfully (YAML → `null`, TOML → `{}`) but rendered a bare degenerate tree
node instead of the same explicit "empty" state a truly empty file gets — `load()` now folds that case
into the empty state too.
2026-08-11 — Caught and fixed an unrelated ratchet regression before it could land: `src/app.css.test.ts`'s
hard-coded-hex-literal guard (CPE-1534) flagged `YamlTomlPreview.svelte` because its own doc comments
wrote "PR #833" — the guard's regex (`#[0-9a-fA-F]{3,8}`) treats "#833" as a false-positive hex literal
(all three digits are valid hex chars). Reworded to "PR 833" (no `#`), matching the convention
NotebookPreview.svelte/LogPreview.svelte already use for the exact same reason.
2026-08-11 — Re-verification after all nine findings fixed: `npm run check` — 0 errors, 0 warnings.
`npx vitest run` (full suite) — **286 files, 3594 tests passed, 0 failed** (up from the 3556 baseline:
+38 new tests — TOML findings #2-#5 = 8, YAML findings #1/#6/#7/#8 = 25, component findings #9/#10 +
block-scalar end-to-end = 5). Re-verified `samples/text/config.{toml,yaml}` still parse cleanly under
the stricter rules (a throwaway verification test, deleted before commit, confirmed both). Pushed to
the existing branch/PR #833 — no new PR, per the coordinator's explicit instruction.
