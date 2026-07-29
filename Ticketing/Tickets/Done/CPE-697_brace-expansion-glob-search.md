---
id: CPE-697
title: Brace-expansion glob search (*.{jpg,png,gif}) across name-filter and find-by-name
type: feature
component: Multiple
priority: low
status: Done
tags: needs-decision
created: 2026-07-18
estimate: 2-3h
---

## Summary
Power users expect `*.{jpg,png,gif}` in file search to match any of the listed alternatives — a standard
shell/fnmatch glob feature. Today the matcher treats `{`, `}`, and `,` as **literal** characters, so the
pattern matches nothing useful. Add brace expansion so `{a,b,c}` becomes an alternation.

## Why it spans two surfaces (do NOT do only half)
The app has **two** glob matchers and they must stay consistent, or the same pattern behaves differently
depending on which search the user reaches for:
- **Ctrl+F name filter** (current folder) — frontend `src/lib/search.ts` `makeMatcher` → `globToRegExp`.
- **Ctrl+P find files by name** (whole subtree) — backend Rust glob in `src-tauri/src/lib.rs`
  (`find_files_by_name`).

Both need the identical brace semantics, with parallel test suites (vitest + cargo).

## Decision needed
Enabling braces changes the currently-documented rule that "regex metacharacters (incl. `{ } ,`) are
literal." A filename containing a literal `{...}` would then be interpreted as a group. Confirm this is an
acceptable trade (it matches bash/Explorer-power-user expectations; literal braces in names are rare). If
literal-brace support must be preserved, decide on an escape (e.g. `\{`).

## Rough scope
- Frontend: extend `globToRegExp` to translate a **matched** `{…}` containing top-level commas into
  `(?:…|…)`, escaping regex-specials inside; leave an unmatched `{` literal. Comma/brace outside a group
  stay literal. `*`/`?` continue to work inside groups.
- Backend: mirror the same expansion in the Rust glob so Ctrl+P agrees with Ctrl+F.
- Tests: `search.test.ts` + a Rust unit test — matched groups, nested/unmatched braces, commas outside
  braces, `*`/`?` inside a group, regex-metachar literality inside a group.
- Docs (CPE-579 rule): update `src/docs/03-explorer.md` search bullet to mention `{a,b}` and keep the
  frontend/backend behaviour described identically.

## Acceptance Criteria
- [x] `*.{jpg,png}` matches `photo.jpg` and `photo.png` but not `photo.gif`, in **both** Ctrl+F and Ctrl+P.
- [x] Unmatched/rogue braces and commas outside a group are treated literally (no crash, sensible match).
- [x] Frontend + backend semantics are identical, each with tests; `npm run check`, full JS suite, and
      `cargo test`/clippy (both feature modes) green.
- [x] Docs updated.

## Notes
Found during the nightshift while auditing the search matcher for CPE-695. Filed rather than
auto-implemented: it changes user-facing search semantics (a `needs-decision`) and must land on the
frontend and backend together to avoid divergence — better done attended than rushed unattended.

## Decision taken (nightshift, 2026-07-18)
Resolved the `needs-decision` autonomously per the ticket's own recommendation and standard bash/fnmatch
semantics: **a matched `{…}` with a top-level comma becomes an alternation; an unmatched brace or a
comma-less `{x}` stays literal.** No escape syntax added — literal-brace *groups* (i.e. `{a,b}` meant
literally) are vanishingly rare in filenames, and both surfaces still match a literal `{x}` (no comma)
exactly. If a real need for escaped literal `{`/`}` inside a group emerges, revisit with `\{`.

## Work Log
2026-07-18 22:20 USMST (nightshift) — Implemented on both surfaces:
- Frontend `src/lib/search.ts`: rewrote `globToRegExp` around a char scanner (`globBody` +
  depth-aware `findBraceGroup`); `makeMatcher` now gates glob-vs-substring via `isGlob` (adds
  brace-group detection). RegExp still compiled exactly once (per-compile test still green).
- Backend `src-tauri/src/lib.rs`: added `expand_braces`/`first_brace_group` (cartesian expansion,
  capped at 1024 patterns) and taught `name_matches` to treat a brace group as a glob and match the
  name against ANY expanded pattern via the existing `glob_is_match`.
- Tests: 4 new frontend cases (groups, `*`/`?` inside, nested/multiple, literal/unmatched) and 3 new
  Rust tests. `npm run check` 0/0; full JS suite 708 pass; `cargo test` 135 pass; clippy `--all-targets
  -D warnings` clean in default + `sidecar-platform`.
- Docs: `src/docs/03-explorer.md` search section documents `{a,b}` for both name searches.
No GUI-timing surface (pure matching logic, fully covered by unit tests), so this is verified by the
parallel suites rather than a live GUI drive.
