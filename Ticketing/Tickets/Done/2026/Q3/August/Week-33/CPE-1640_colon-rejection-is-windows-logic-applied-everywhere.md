---
id: CPE-1640
title: "Rename templates containing a colon are refused on Linux and macOS too, where a colon is a perfectly legal filename character"
type: Bug
status: Backlog
priority: Low
component: Backend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent Reviewer of CPE-1623 (PR #828) while checking the cross-platform blast radius of a
Windows-motivated guard. The author had stated plainly that they only tested on Windows, which is exactly the
kind of honest disclosure that lets a reviewer go looking in the right place.

## The gap
`template_escapes_directory()` (`crates/server/src/batch_media.rs:217-222`) rejects any template containing
`:` — and it has **no `#[cfg(windows)]` gate**, so the rule fires identically on all three platforms. The
frontend mirror `templateEscapesDirectory()` in `src/lib/batchMedia.ts` does the same.

The colon rule exists for Windows reasons: `C:foo` is a drive-relative path that resolves against that
drive's current directory, and a colon elsewhere in a path is an NTFS alternate-data-stream separator (see
CPE-1624). Neither applies off Windows:
- On **Linux** (ext4 and friends) `:` is an ordinary filename character.
- On **macOS**, `:` is legal at the raw APFS/HFS+ level; Finder's `/`↔`:` display swap is cosmetic, not a
  filesystem restriction.

So a Linux or macOS user typing an entirely ordinary template — a timestamp like `10:30am-photo`, or
`session:final` — is refused, for a reason that does not exist on their machine.

## Why CI can't catch it
The rule is deliberately uniform, so it behaves identically on all three legs of the 3-OS matrix and no test
fails anywhere. This is a false positive that only a human (or a Unix-side UAT pass) would notice — worth
remembering as a pattern: a guard that is *consistently* wrong is invisible to a matrix that only checks for
*inconsistency*.

## Fix
Gate the colon rule to Windows (and keep it in the frontend mirror only when the host is Windows, so the two
stay in lockstep — the reviewer confirmed they currently agree, and that property must survive).

**Safe to do:** the colon rule is a friendly early check, not the enforcement point. The real guarantee is
`output_escapes_input_dir`, which runs on the fully-substituted output path and is unconditional. Relaxing
the template check on Unix therefore cannot reopen a containment escape — verify that claim rather than
assuming it, and record the check in the work log.

## Acceptance criteria
- A template containing `:` is accepted on Linux/macOS and still refused on Windows; tests gated per platform.
- Backend and frontend agree on every case, on every platform.
- Containment is demonstrably unaffected: an escaping output is still refused on all platforms, including via
  a hand-built `PlannedItem` that skips `plan()` entirely.
- CPE-1624's ADS concern is untouched — the reviewer confirmed the colon rule never closed it anyway (it is
  template-only and does not inspect a hand-built output path).

**Conflict surface:** `crates/server/src/batch_media.rs`, `src/lib/batchMedia.ts`, plus tests. Small.
Overlaps CPE-1623/CPE-1624's files — land those first.
