---
id: CPE-635
title: Backend tag store (persisted JSON + commands)
type: feature
component: Backend
priority: medium
status: Done
tags: ready
estimate: 1-2h
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

## Summary
First child of CPE-614 (tags & smart folders). An app-side organisational store: user tags + a colour
label per path, persisted as `tags.json` in the config dir (the filesystem is never touched). Pure
model helpers are cargo-tested; the commands are thin I/O.

## Acceptance Criteria
- [x] `TagEntry {tags, label}` + `TagStore = BTreeMap<path, entry>`; pure `tag_store_set` (trim/dedupe/
      sort/prune-empty) and `tag_store_counts` (most-used first) — unit-tested.
- [x] `read_tags_from`/`write_tags_to` round-trip via `tags.json`; missing/corrupt ⇒ empty store.
- [x] Commands `load_tags`, `set_tags`, `tag_counts` registered in `generate_handler!`.
- [x] cargo tests + clippy (both feature modes) clean.

## Resolution
Added the store to `src-tauri/src/lib.rs` (mirrors the settings pattern). 3 cargo tests. Rename-follows
(re-key tags when a path is renamed) deferred to the assign/wiring child.

## Work Log
2026-07-18 (dayshift) — Built the tag-store foundation.
