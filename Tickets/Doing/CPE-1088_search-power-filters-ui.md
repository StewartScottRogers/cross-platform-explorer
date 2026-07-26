---
id: CPE-1088
title: "Search power-filters in the folder search box (size:/date:/type:/boolean DSL, client-side)"
type: feature
component: Frontend
priority: high
status: Doing
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
- [ ] `makeEntryMatcher` supports `size:`/`date:`|`modified:`/`type:`/`ext:`/`path:`/bare-name + `OR`/`NOT`/`-`/
      parentheses, ANDing by default; empty query matches all; deep-nesting can't stack-overflow.
- [ ] Typing e.g. `size:>1mb type:image` in the folder search box filters the current folder live (verified in
      the running app); plain-text and glob queries still behave exactly as before (no regression).
- [ ] New `src/lib/entrySearch.test.ts` (vitest) covers each filter kind + boolean precedence + a fixed-`now`
      relative-date case + NaN/garbage token + empty query. `npx vitest run src/lib/entrySearch.test.ts`
      green; the existing search.test.ts (if any) still green.
- [ ] `npm run check` (svelte-check + tsc) clean. In-app search docs updated; `sectionDocs.test.ts` green.

## Work Log
2026-07-26 (workshift, GUI) — Filed by the Foreman: user picked "search power-filters" as the first GUI
surface after the headless epics. Client-side TS port of the four Rust filter modules into the instant folder
filter (DirEntry already has size/modified/ext, so no backend change). The Rust modules remain the backend
(feature-gated index) implementation; this TS matcher is the shipping folder-filter implementation — keep
their DSL semantics in sync (documented). Final visual verification via build → install sidecar → run.
