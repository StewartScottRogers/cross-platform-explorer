---
id: CPE-1355
title: "Linux drive-type classification: real removable/fixed detection instead of hardcoded \"fixed\""
type: Feature
status: Done
priority: Low
component: src-tauri
tags: [ready]
epic: CPE-716
created: 2026-08-05
closed: 2026-08-06
---

## Problem

`src-tauri/src/lib.rs` `drive_type_impl` on non-Windows is a stub:
```rust
#[cfg(not(windows))]
fn drive_type_impl(_path: &str) -> String { "fixed".to_string() }  // "best-effort until unix ... lands"
```
It feeds the shipped `drive_type` command + the `eject_guard` safety gate; the Sidebar's eject affordance only
shows for `"removable"`. So on Linux a real USB drive is always misclassified as fixed. This is a genuinely
unbuilt classifier (a documented follow-up), not a design choice.

## Do (scope: Linux first)

Implement a real Linux classifier: resolve the mount point for `path` from `/proc/mounts`, map it to its block
device, and read `/sys/block/<dev>/removable` (1 = removable) — pure stdlib, no new dep. Return
`"removable"`/`"fixed"` accordingly; fall back to `"fixed"` on any read failure (never panic).
Make the core logic a **pure function over injected synthetic mount-table + removable-flag strings** (mirror
how `eject_guard`/`eject_drive_seam` were made hardware-free-testable in `lib.rs`), so it's unit-testable with
no real hardware. Leave macOS as `"fixed"` for now (no `/proc`; a later slice — note it).

## Acceptance criteria

- Pure classifier fn unit-tested with synthetic `/proc/mounts` + `/sys/block/*/removable` fixtures covering:
  a USB (removable=1) → `"removable"`, an internal disk (removable=0) → `"fixed"`, unresolvable path → `"fixed"`.
- `#[cfg(target_os = "linux")]` wiring; macOS/other still `"fixed"`. No panic on any malformed proc/sys input.
- clippy clean. No new deps.

## Notes

CI-only verifiable on the ubuntu leg (can't run on the Windows dev box — same accepted constraint as the HEIC
macOS path). Does NOT unlock actual ejecting on Linux (`perform_eject` still returns "not supported yet") —
only correct classification/badging. Surfaced by the 2026-08-05 frontier scan (ranked #3). Epic CPE-716.

## Work Log
- 2026-08-06 (sprint): PR #650 merged. Real Linux drive-type classification (pure classify_drive_type_from_proc fn over /proc/mounts + /sys/block/*/removable seam; linux-cfg wrapper; macOS still fixed; Windows untouched). Reviewer CHANGES-then-APPROVE (caught unpartitioned whole-disk nvme/mmcblk reduction bug: nvme0n1->nvme0n; fixed to strip only pN suffix) + UAT PASS (mutation-proven, nvme reduction verified). 132 pure-fn tests run on Windows; linux wrapper CI-compiled on ubuntu (Backend-ubuntu green). Attended: real USB on Linux hardware.
