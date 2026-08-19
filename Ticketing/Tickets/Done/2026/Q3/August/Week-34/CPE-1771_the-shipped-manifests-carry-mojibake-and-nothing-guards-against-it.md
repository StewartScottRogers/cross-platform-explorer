---
id: CPE-1771
title: The shipped manifests carry mojibake, and nothing guards against it recurring
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-17
closed:
---

## Problem

Found by the **PR #927 (CPE-1752) review**, which ran its own tree-wide scan rather than trusting the
ticket's claim that `dispatch.rs` was the only affected file. Measured, on `main`:

| File | `â€` occurrences |
|---|---|
| `src-tauri/Cargo.toml` | **35** |
| `src-tauri/tauri.conf.json` | **1** |

Same signature as CPE-1752 — UTF-8 read as CP1252 by a PowerShell `Get-Content`/`Set-Content` round-trip.

CPE-1752's scan was scoped to `crates/`, `src-tauri/src/`, `src/`, and `docs/`, so it satisfied its own
acceptance criterion honestly. These two files sit inside `src-tauri/` but **outside `src-tauri/src/`** —
in the gap between the scanned directories. The ticket's Problem statement claim that "a tree-wide scan
found no other affected file" is true only within that narrower boundary.

## Why these two files in particular

They are the **release manifests**. `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` are two of the
three files that must stay version-synced on every release, and the same PowerShell round-trip that caused
this already **blocked release 0.57.66** once by adding a BOM to them — see commit `86888aed`, "strip the
BOM I added to the manifests". The BOM was removed; **the mojibake it introduced alongside was not.**

So this is not merely cosmetic corruption in a comment. It is corruption in the files whose integrity the
release depends on, from a known incident, sitting in the blind spot of the scan that was supposed to find
it.

## What to do

1. **Repair both files byte-exactly.** Use `iconv`/`sed`/`python`/an editor tool — **never** a PowerShell
   text round-trip, which is the cause. Verify with `git diff --numstat`: expect ~35 and ~1 changed lines,
   **not** a whole-file rewrite. A whole-file count means it was re-encoded again.
2. **Check what the corrupted bytes are actually in.** A mangled character inside a TOML comment is
   cosmetic; one inside a `description`, `authors`, or a `tauri.conf.json` string **ships to users** in
   installer metadata and the app's own about surface. Report which, per occurrence, before deciding it is
   harmless.
3. **Build the guard, which CPE-1752 deliberately deferred.** Its Notes section framed a CI byte-signature
   check as an out-of-scope follow-up ("File it separately if you agree") — this is that ticket. A CI job
   that fails on the mojibake signature (`c3 a2 e2 82 ac` and friends: `Ã`, `Â `, `â€™`, `â€œ`, `â€¦`) and on
   a UTF-8 BOM, **across the whole repo**, not a hand-listed set of directories. The directory list is
   exactly what let these two files through.
4. **Watch for false positives.** The #927 review verified two: `src/lib/i18n.ts:5320` contains a legitimate
   Portuguese `"NÃO"`, and `src/lib/docs.ts` / `epicsQueueLayout.test.ts` contain literal `﻿`
   BOM-stripping regexes. The guard must not red on those. Allowlist them by exact location with a recorded
   reason — not by weakening the pattern.

## Acceptance criteria

- [ ] Both manifests are repaired; 0 mojibake sequences remain, verified against the **git blob** and not
      just the checkout.
- [ ] Neither file has a BOM, verified against the blob (`git show :file | head -c 3 | xxd`).
- [ ] `git diff --numstat` shows a targeted edit, not a re-encode.
- [ ] Each repaired occurrence is reported with what it was inside (comment vs shipped string value), and
      anything user-visible is called out.
- [ ] A CI guard fails on a mojibake byte signature or a BOM **anywhere in the repo**. Demonstrate by
      planting the sequence in a scratch file and showing the guard red, then removing it.
- [ ] The two known false positives stay green, allowlisted by location with a reason.
- [ ] The app still builds and `cargo metadata` parses the repaired `Cargo.toml`.

## Notes

Found by the Reviewer on **PR #927 / CPE-1752**, 2026-08-17, during the batched sprint. Related: CPE-1752
(the `dispatch.rs` repair, which was correctly scoped and passed), commit `86888aed` (the BOM incident on
these same two files), CPE-1733 (where a worker caught the same round-trip live via `git diff --numstat`).
