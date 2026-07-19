---
id: CPE-697
title: Brace-expansion glob search (*.{jpg,png,gif}) across name-filter and find-by-name
type: feature
component: Multiple
priority: low
status: Open
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
- [ ] `*.{jpg,png}` matches `photo.jpg` and `photo.png` but not `photo.gif`, in **both** Ctrl+F and Ctrl+P.
- [ ] Unmatched/rogue braces and commas outside a group are treated literally (no crash, sensible match).
- [ ] Frontend + backend semantics are identical, each with tests; `npm run check`, full JS suite, and
      `cargo test`/clippy (both feature modes) green.
- [ ] Docs updated.

## Notes
Found during the nightshift while auditing the search matcher for CPE-695. Filed rather than
auto-implemented: it changes user-facing search semantics (a `needs-decision`) and must land on the
frontend and backend together to avoid divergence — better done attended than rushed unattended.
