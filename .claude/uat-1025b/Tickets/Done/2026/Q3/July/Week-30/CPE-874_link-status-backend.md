---
id: CPE-874
title: Link status backend — symlink target + broken-link detection
type: feature
component: Sidecar
priority: low
status: Done
tags: ready
epic: CPE-715
created: 2026-07-21
closed: 2026-07-21
---

## Summary
Backend detection slice for the Link Forge epic (CPE-715): tell whether a path is a symlink, where it
points, and whether that target is missing (a **broken link**) — the primitive a future "resolves to"
indicator + broken-link badge (CPE-803/804 GUI) render over.

## Acceptance Criteria
- [x] `cpe_server::links::link_status(path) -> LinkStatus { is_symlink, target, broken }`: never fails; a
      non-symlink/unreadable path reports `is_symlink=false`; `broken` follows the link and is true only for
      a symlink whose target no longer resolves. Unit-tested (skip-on-unprivileged-Windows pattern).
- [x] Thin `link_status` command registered.
- [x] cargo test + clippy `-D warnings` green.

## Work Log
- 2026-07-21 (autonomous) — Backend primitive for CPE-715; the "New Link" UI + badges (CPE-803) and repair +
  junctions (CPE-804) remain the GUI/attended tail.
