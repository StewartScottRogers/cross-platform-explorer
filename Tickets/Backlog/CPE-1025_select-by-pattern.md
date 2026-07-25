---
id: CPE-1025
title: Select-by-pattern engine (pure model)
type: feature
component: Backend
priority: medium
tags: ready
epic: CPE-711
created: 2026-07-25
status: Backlog
---

## Summary
Headless core for the Advanced Selection epic (CPE-711): given a directory listing and a selection query,
return which entries match — the tested engine behind a future "Select… (glob / by extension / invert)"
command. A **pure** function in a new `cpe_server::selection` module. No filesystem access — it operates on
the entry names/kinds it's handed.

Query kinds:
- `Glob(pattern)` — shell-style `*`/`?` over the entry name (case-insensitive), reuse the app's existing
  glob matcher if one exists in `cpe-server`; otherwise a small self-contained matcher.
- `Extension(list)` — every name whose extension is in the (lower-cased, no-dot) list; folders never match.
- `AllFiles` / `AllFolders` — by kind.
- `Invert(inner)` — the complement of another query over the same listing.

## Acceptance Criteria
- [ ] `select(entries, &query) -> Vec<usize>` (indices into `entries`, in listing order) for Glob /
      Extension / AllFiles / AllFolders / Invert.
- [ ] Glob is case-insensitive and anchored to the whole name; `*.rs`, `a?b*`, and literal names work.
      Extension match is case-insensitive; a folder never matches Extension/AllFiles.
- [ ] `Invert` returns exactly the entries the inner query did **not** select (complement, order preserved).
- [ ] `SelQuery` + entry input derive `serde::Serialize` + the `specta` cfg derive like sibling types.
- [ ] Pure — no fs; clippy clean both feature modes; ≥6 unit tests incl. invert + mixed file/folder listings.

## Notes
New module `crates/server/src/selection.rs`, declared in `crates/server/src/lib.rs`. **Grep cpe-server first
for an existing glob/wildcard matcher** (the wildcard search feature CPE-052 may already have one) and reuse
it rather than adding a second — name it in the work log. No new dependencies.

## Work Log
- Built `crates/server/src/selection.rs`: `SelEntry { name, is_dir }`, `SelQuery { Glob, Extension,
  AllFiles, AllFolders, Invert }` (`#[serde(tag = "kind", rename_all = "snake_case")]`, matching the
  `AppliesTo` pattern in `shell_menu.rs`), and `select(entries, &query) -> Vec<usize>`. Declared
  `pub mod selection;` in `lib.rs` next to `pub mod links;`.
- **Glob-reuse decision:** grepped `crates/server/src` for an existing wildcard matcher and found
  `name_search.rs::glob_is_match` (CPE-603/697/666, wildcard search) — same anchored `*`/`?` two-pointer
  algorithm this ticket needs. Did **not** call it: it's a private fn, and its public wrapper
  `name_matches` folds in a substring fallback for non-glob queries, which would make a literal
  `SelQuery::Glob("readme.txt")` match any name merely *containing* "readme.txt" instead of the anchored
  exact-match semantics this ticket specifies. Making the private fn `pub(crate)` would touch a file
  outside this ticket's allowed scope, so `selection.rs` ships its own small self-contained matcher
  (same algorithm shape, no regex, no new dependency) — documented in the module doc comment.
- Tests: 8 unit tests in `selection.rs` (glob case-insensitive+anchored, `?` wildcard, literal-glob exact
  match, extension case-insensitive + folder exclusion, AllFiles/AllFolders kind split, Invert exact
  complement + order preservation, Invert of AllFiles, empty listing/no-match). `cargo test -q selection`
  → **12 passed; 0 failed** (8 in `selection::tests` + 4 pre-existing tests elsewhere whose names contain
  "selection"), 0 failed.
- `cargo clippy --all-targets -- -D warnings` → clean. `cargo clippy --all-targets --all-features -- -D
  warnings` → clean.
- Touched only `crates/server/src/selection.rs`, `crates/server/src/lib.rs`, and this ticket file.
- **UAT fix (PR #342):** internally-tagged serde (`tag = "kind"`) can't represent this enum's newtype
  (`Glob(String)`, `Extension(Vec<String>)`) or recursive (`Invert(Box<SelQuery>)`) variants — it fails
  to serialize and won't even compile a `Serialize` call (E0275 overflow via the recursion). Switched to
  the default **externally-tagged** representation (dropped `tag = "kind"`, kept `rename_all =
  "snake_case"`) and added `serde::Deserialize` so the type actually crosses the wire. Added a unit test
  that round-trips every variant through `serde_json` (`from_str(&to_string(&q)) == q`). Re-ran:
  `cargo test -q selection` → **13 passed; 0 failed** (was 12); both clippy modes still clean.
