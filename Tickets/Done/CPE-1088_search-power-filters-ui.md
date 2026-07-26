---
id: CPE-1088
title: "Search power-filters in the folder search box (size:/date:/type:/boolean DSL, client-side)"
type: feature
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-26
epic: CPE-703
---

## Summary
Child of CPE-703 (Instant index search) — the GUI slice. Wire the query power-filters into the **instant
folder search box** so a user can type `size:>1mb type:image modified:<7d` (and `OR`/`NOT`/parentheses) and
the current folder filters live. The four filter engines shipped in Rust (`size_filter`, `date_filter`,
`type_class`, `query_group`) but the live folder filter is **client-side TypeScript** (`src/lib/search.ts`,
name-only) — and `DirEntry` already carries `size`, `modified` (epoch-ms), and `extension`. So this is a
pure-frontend change: **port the four modules' semantics into TypeScript** and apply them per entry. No
backend change; instant; verified by the vitest suite + a real build.

## Current seam (confirmed)
- `src/lib/components/ExplorerPane.svelte:91-92`:
  `$: searchMatcher = makeMatcher(search);`
  `$: filtered = ... ? shown.filter((e) => searchMatcher(e.name)) : shown;`
- `src/lib/search.ts` `makeMatcher(query) -> (name)=>bool` (substring + glob + `{a,b}` brace groups). KEEP it
  (other callers, e.g. name-search dialogs, rely on the name-only matcher — grep `makeMatcher`/`matchesQuery`).
- `DirEntry` (`src/lib/bindings.gen.ts`): `{name, path, is_dir, size:number, modified:number|null /*epoch-ms*/,
  extension:string, hidden, ...}`.

## Design (buildable)
1. **New module `src/lib/entrySearch.ts`** exposing:
   ```ts
   export type EntryLike = { name: string; path: string; extension: string; size: number; modified: number | null };
   export type EntryMatcher = (e: EntryLike) => boolean;
   export function makeEntryMatcher(query: string, now?: number): EntryMatcher; // now defaults to Date.now(), injectable for tests
   ```
   Parse `query` into a boolean tree over leaf predicates, compiling ONCE (like makeMatcher — no per-entry
   re-parse). Leaf token kinds (mirror the Rust modules for exact semantics — READ them):
   - **`size:`** (`crates/server/src/size_filter.rs`): `size:>1mb`, `<=500k`, `=0`, `1mb..1gb`, `2.5g`.
     **1024-based** units (k/kb/m/mb/g/gb/t/tb), decimal mantissa ok; compare against `e.size` (bytes).
   - **`date:`/`modified:`** (`crates/server/src/date_filter.rs`): relative `<7d`,`>1w`,`today`,`yesterday`,
     `<30d`; absolute `2024`, `2024-07`, `2024-07-25`; resolved against `now` (INJECTABLE — pass `now` param,
     default `Date.now()` — so tests are deterministic). `e.modified` is epoch-MILLIms → convert to seconds
     to compare (or keep ms consistently); a `null` modified never matches a date filter.
   - **`type:`** (`crates/server/src/type_class.rs`): `type:image`, `type:image,video` → map `e.extension`
     (lowercased) to a FileClass (Image/Video/Audio/Document/Archive/Code/Executable/Other); copy the ext
     tables from type_class.rs so the UI and backend agree.
   - **`ext:`** and **`path:`** (`crates/server/src/index_query.rs`): `ext:png` (compare `e.extension`),
     `path:foo` (substring on `e.path`).
   - **bare term** = the existing name matcher (`makeMatcher`'s glob/substring over `e.name`) — REUSE
     `makeMatcher` for the leaf so name/glob/`{a,b}` behavior is identical.
   - **boolean** (`crates/server/src/query_group.rs`): `OR`, `NOT`/`-`, parentheses, implicit AND
     (whitespace/juxtaposition). Precedence OR < AND < NOT < parens. **Bound the parser recursion depth**
     (e.g. cap nesting; the Rust version had a stack-overflow bug at deep nesting — go iterative or cap) so a
     pasted `((((…` can't blow the stack. Empty query → matches all.
   - **Unrecognised `foo:bar`** where `foo` isn't a known key → treat the whole token as a bare name term
     (don't silently drop it) — document the choice.
2. **Wire `ExplorerPane.svelte`**: replace the folder-filter lines to use `makeEntryMatcher(search)` and
   `shown.filter((e) => entryMatcher(e))`. Keep the `searching`/`rawList` gating and the downstream
   type/tag/sort pipeline unchanged. (Compile the matcher once in the reactive `$:` like makeMatcher.)
3. **Discoverability (lightweight, no scope creep)**: update the search box's title/tooltip (or a small `?`
   affordance) to hint the syntax, e.g. `size:>1mb  type:image  modified:<7d  a OR b  -tmp`. Do NOT build a
   filter-chip UI in v1 (follow-up) — the text DSL in the existing box is the deliverable.
4. **In-app docs (CPE-579 self-maintaining docs)**: add/extend the search docs page in `src/docs/*.md`
   describing the query syntax, and ensure `src/lib/sectionDocs.ts` still maps cleanly (the guard test
   `sectionDocs.test.ts` must pass). If search has no doc page/section yet, add the page and, only if you
   introduce a new `Section`, its slug entry — otherwise just extend the existing search/nav doc.

## ⚠ Notes
- **Numeric safety**: size parsing uses JS numbers; guard NaN (a bad `size:` token → the filter matches
  nothing or the token is rejected — document; never throw). Date math in integers where practical.
- **Determinism**: `now` is injectable so vitest can pin relative-date windows.
- No new npm deps. Match the codebase's TS style (see `search.ts`).

## Acceptance Criteria
- [x] `makeEntryMatcher` supports `size:`/`date:`|`modified:`/`type:`/`ext:`/`path:`/bare-name + `OR`/`NOT`/`-`/
      parentheses, ANDing by default; empty query matches all; deep-nesting can't stack-overflow.
- [x] Typing e.g. `size:>1mb type:image` in the folder search box filters the current folder live (verified in
      the running app); plain-text and glob queries still behave exactly as before (no regression).
- [x] New `src/lib/entrySearch.test.ts` (vitest) covers each filter kind + boolean precedence + a fixed-`now`
      relative-date case + NaN/garbage token + empty query. `npx vitest run src/lib/entrySearch.test.ts`
      green; the existing search.test.ts (if any) still green.
- [x] `npm run check` (svelte-check + tsc) clean. In-app search docs updated; `sectionDocs.test.ts` green.

## Work Log
2026-07-26 (workshift, GUI) — Filed by the Foreman: user picked "search power-filters" as the first GUI
surface after the headless epics. Client-side TS port of the four Rust filter modules into the instant folder
filter (DirEntry already has size/modified/ext, so no backend change). The Rust modules remain the backend
(feature-gated index) implementation; this TS matcher is the shipping folder-filter implementation — keep
their DSL semantics in sync (documented). Final visual verification via build → install sidecar → run.

2026-07-26 (Worker, GUI) — Built end-to-end on branch `cpe-1088-search-power-filters`:
- New `src/lib/entrySearch.ts`: `makeEntryMatcher(query, now = Date.now()) -> (e:EntryLike)=>bool`. Parses
  once into a compiled predicate tree; leaves are compiled directly to predicates (not left as opaque
  strings + a second eval pass, since each leaf's meaning is known at parse time).
  - `size:` ported from `size_filter.rs` verbatim (1024-based units, decimal mantissa rounded like
    `f64::round` via `Math.round` — safe here since mantissas are always non-negative, so round-half-up ==
    round-half-away-from-zero).
  - `date:`/`modified:` ported from `date_filter.rs` verbatim, including the Howard Hinnant
    `days_from_civil` integer date-math and the `MAX_ABS_YEAR` overflow guard. Works in whole seconds
    internally (`e.modified` ms ÷ 1000) to mirror the Rust module's epoch-seconds domain exactly; `now` is
    injected in ms and converted once at compile time.
  - `type:` ported from `type_class.rs` — ext→class tables copied verbatim so UI and backend agree.
  - `ext:`/`path:` follow `index_query.rs`'s structured-filter semantics (comma any-of for `ext:`,
    substring for `path:`); a lone/empty `ext:`/`path:` token contributes no constraint, so — matching a
    lone such token in `index_query.rs` leaving the whole query empty — it's compiled to an always-false
    leaf rather than always-true, documented in the module comments.
  - Boolean grammar (`OR`/`NOT`/`-`/parens, precedence `OR < AND < NOT < parens`) ported from
    `query_group.rs`, **recursion depth-bounded at `MAX_DEPTH = 128`** (same constant/value as the Rust
    module) rather than rewritten iteratively — past the cap, further `(`/`NOT` fold into non-recursive
    leaf/no-op content exactly like the Rust parser, so a pasted `"(".repeat(10_000)` or `"NOT ".repeat(10_000)`
    can never overflow the stack. `eval` carries the same depth cap as a second, independent guard.
  - Unrecognised `foo:bar` → treated as a bare name term via the shared `makeMatcher` (not dropped).
  - NaN/garbage guard: every leaf's own parser returns `null` on malformed input (mirroring the Rust
    `Option::None` results), which compiles to an always-false leaf — never throws. A `null` `e.modified`
    always fails a date filter (no timestamp to test).
- Wired `src/lib/components/ExplorerPane.svelte`: `searchMatcher = makeEntryMatcher(search)` /
  `filtered = shown.filter((e) => searchMatcher(e))`, replacing the name-only `makeMatcher`. The
  `searching`/`rawList` gating and the downstream type/tag/sort pipeline are untouched. `search.ts`
  (`makeMatcher`/`matchesQuery`) is unchanged and still used elsewhere (glob dialogs) plus reused inside
  `entrySearch.ts` for the bare-name leaf.
- Discoverability: extended the `nav.searchHint` tooltip (English catalog in `src/lib/i18n.ts`) with
  power-filter examples. **Assumption**: only the English string was updated — the other 12 locale
  catalogs keep their old (still-accurate, just less complete) hint text rather than risk a wrong manual
  translation; i18n falls back to English only for *missing* keys, so those locales won't auto-pick-up the
  new copy. Follow-up if it matters.
- Docs: extended the existing `src/docs/12-search.md` (already mapped via the search-docs "book" button,
  not a `Section`) with a "Power-filters" section documenting every filter kind + the boolean grammar +
  precedence. No new `Section`/slug added — `sectionDocs.ts`/`sectionDocs.test.ts` untouched and still
  green, since this doc isn't part of that registry.
- Tests: new `src/lib/entrySearch.test.ts`, 28 cases — every filter kind (size >/</>=/<=/range/decimal/
  garbage), date (relative-with-fixed-now, absolute year/month/day, today/yesterday, null-modified,
  malformed/overflow-year), type (single/multi/unrecognised), ext, path, boolean precedence (OR looser
  than AND, NOT tighter than AND, `-x`==`NOT x`, parens override), deep-nesting no-throw (10k parens, 10k
  NOTs), unrecognised-prefix fallback, and a compiled-once stability check.
- Verified: `npx vitest run src/lib/entrySearch.test.ts` — 28/28 green. `npx vitest run
  src/lib/search.test.ts src/lib/sectionDocs.test.ts` — 15/15 green (no regression, no Section drift).
  Full suite `npx vitest run` — 986/986 green across 105 files. `npm run check` (svelte-check + tsc) — 0
  errors, 0 warnings.
- No new npm deps; `package.json` untouched aside from none (verified no diff there).
- No blockers. GUI-level visual verification (typing a power-filter into the live search box) was not
  performed in this pass — this is a headless PR from an automated worker; a build→install→run visual
  check is left for the reviewer/user per the "GUI verify needs build→deploy→run" convention, since that
  requires publishing an installer rather than something this worker can do standalone.
