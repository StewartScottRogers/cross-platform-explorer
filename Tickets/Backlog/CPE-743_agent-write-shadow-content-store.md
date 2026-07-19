---
id: CPE-743
title: Agent Watch — host shadow-content store + before/after capture for writes
type: feature
component: Backend
priority: high
status: Open
tags: needs-decision
created: 2026-07-19
epic: CPE-727
estimate: 4-5h
---

## Summary
Child of CPE-727 and its **foundation**. To show what an agent write changed, we need the file content
*before* the change — which the FS watcher can't provide (its `modified` event fires after the write). Add a
bounded, in-memory **shadow-content store** in the host watcher (`src-tauri`): cache the last-seen text of
files under the watched tree, and on each `created`/`modified` event pair the new content with the cached
previous (empty for a create), emitting a before/after record on a new channel alongside `fs-activity`.

## Scope
- On watch start and on each mutation, capture file content into a per-path cache keyed by absolute path.
- On `modified`: emit `{path, before, after}` (before = cached prior text, after = new text), then update the
  cache. On `created`: before = "".
- **Text-only + bounded:** skip non-UTF8/binary and files over a per-file byte cap (~1 MiB); bound the whole
  store by total bytes + entry count with LRU eviction. Never block the watcher or the terminal.
- New emit channel (e.g. `ai-console://fs-diff`) OR extend the fs-activity payload — decide in scope.
- Idle-by-default: nothing allocated until watching starts; cleared on stop (off means off).

## Decisions needed (resolve at pickup — best-guess if unattended)
- Channel: separate `fs-diff` event vs. enriching `fs-activity` (lean: **separate**, keeps activity lean).
- Caps: per-file byte cap + total store cap values; eviction policy (lean: LRU by last-touch).
- Whether to compute the diff host-side or ship before/after and diff in the frontend (lean: **ship
  before/after**, diff in frontend via `diff.ts` — keeps the host dumb, reuses tested code).

## Acceptance
- [ ] While watching, each created/modified file emits a before/after record (before empty for creates).
- [ ] Binary/oversized files are skipped cleanly; the store is bounded and evicts under pressure.
- [ ] Store is empty/idle when not watching and cleared on stop; watcher/terminal never blocked.
- [ ] cargo-tested: capture/pair/evict/skip-binary logic (no exact-fs-size asserts — 3-OS matrix).

## Notes
Foundation for CPE-744/745/746. The shadow store could later back checkpoint/rollback (CPE-732) — out of
scope here, but keep the design amenable.
