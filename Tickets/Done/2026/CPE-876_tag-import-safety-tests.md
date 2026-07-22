---
id: CPE-876
title: Lock in tag-store import safety + rename-tag edge cases with tests
type: chore
component: Server
priority: low
tags: ready
epic: CPE-614
created: 2026-07-21
closed: 2026-07-21
status: Done
---

## Summary
`crates/server/src/tags.rs` had 16 pub fns but only covered the happy paths. The most important untested
guarantee was the **import safety ordering**: `import` parses the incoming JSON *before* it reads or writes
the persisted store, so a malformed import can never clobber the user's existing tags — but nothing pinned
that ordering, so a future refactor (read-then-parse) could silently introduce data loss. Also uncovered:
`rename_tag`'s self-rename no-op / merge-into-existing dedupe.

Added regression tests only — no behavior change.

## Acceptance Criteria
- [x] `import` on malformed JSON returns a clear error **and** leaves the persisted store byte-for-byte intact.
- [x] `import` on valid JSON unions non-destructively and persists (existing tags kept, non-empty label wins).
- [x] `rename_tag(x → x)` is a no-op; `rename_tag` into a tag the path already has collapses to one (no dup).
- [x] `cargo test` + `cargo clippy --all-targets -D warnings` green in `cpe-server`.

## Work Log
- 2026-07-21 (autonomous) — Reviewed the module for real defects (none found — the ordering is already
  correct), then added 3 tests over the `HeadlessCtx` harness to lock the guarantees in. 26/26 tags tests
  pass; clippy clean.
