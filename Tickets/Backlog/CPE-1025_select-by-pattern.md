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
