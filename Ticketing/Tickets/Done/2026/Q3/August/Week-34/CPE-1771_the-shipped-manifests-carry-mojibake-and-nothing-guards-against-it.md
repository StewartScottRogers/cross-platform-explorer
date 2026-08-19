---
id: CPE-1771
title: The shipped manifests carry mojibake, and nothing guards against it recurring
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-17
closed: 2026-08-19
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

## Resolution

Both manifests repaired byte-exact (Python, never PowerShell) and a whole-repo mojibake+BOM guard added.

**What was corrupted, and where:**
- `src-tauri/Cargo.toml`: 34 em-dash (`—`) + 3 ellipsis (`…`) occurrences, all 37 inside `#`-comments —
  cosmetic, never shipped to a user or parsed by Cargo.
- `src-tauri/tauri.conf.json`: 1 em-dash occurrence, inside the `plugins.cli.description` string —
  **user-visible**: this is the description Tauri's CLI plugin shows for `--help` on the built app.
- `CLAUDE.md` (found while measuring the guard's blast radius, not named in the original ticket, but
  repaired here since leaving it broken would red the new whole-repo guard on push): 19 em-dash + 1 arrow
  occurrence, plus a UTF-8 BOM at byte 0. All in prose; CLAUDE.md ships in the repo and is read by every
  AI session, so treated as in-scope for "shipped" in spirit even though not code/config.

Verified against the git blob: 0 mojibake sequences and 0 BOM in all three files' `HEAD` blobs.
`git diff --numstat` for each repair showed only the changed-line count (35/1/21), never a whole-file
rewrite. `cargo metadata --no-deps` parses the repaired `Cargo.toml` cleanly.

**The guard** (`src/lib/mojibakeGuard.ts` + `src/lib/mojibakeGuard.test.ts`): matches a Latin-1-supplement
lead character (`Ã`/`Â`/`â`) immediately followed by a CP1252 0x80-0x9F artifact character or NBSP — not
the lead character alone, which is an ordinary letter in French/Portuguese/Romanian text (confirmed
against `src/lib/i18n.ts`'s real non-English strings). Walks the whole repo minus build output, vendored
deps, and (documented, see below) `Ticketing/`. Allowlists the two named false positives
(`src/lib/i18n.ts:5320`, `src/lib/docs.ts:25` + `epicsQueueLayout.test.ts:24,35`) by exact location with a
reason, plus three lines of **real** corruption in `crates/server/src/dispatch.rs` that CPE-1752 missed —
`crates/` was off-limits during this sprint slot (a concurrent worker was live in it), so those are
allowlisted with an honest "tracked as CPE-1783" reason, not misrepresented as false positives.

Red-proofed: reverted the manifest-repair commit, re-ran the guard test — it failed, listing all 38
expected offenders by file:line. Restored the repair, guard passed again.

**Follow-ups filed rather than silently expanded into this ticket:**
- CPE-1783 — repair the three `dispatch.rs` arrows and remove their allowlist entries.
- CPE-1784 — `Ticketing/Tickets` carries 683 mojibake occurrences across 14 files and a BOM in 12 files;
  excluded from this guard's scan (measured as a much larger, separate cleanup), with the exclusion
  documented in code pending that ticket.

`npm run check` and the full `vitest` suite (314 files / 4095 tests) pass.

## Work Log

- 2026-08-19: Repaired `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `CLAUDE.md` byte-exact via
  Python. Built `src/lib/mojibakeGuard.ts` + `.test.ts`. Red-proofed by reverting and restoring the
  manifest fix. Filed CPE-1783 (dispatch.rs residual) and CPE-1784 (Ticketing/ cleanup) as follow-ups.
  `npm run check` + full test suite green. PR opened, CI watched to green.

## Notes

Found by the Reviewer on **PR #927 / CPE-1752**, 2026-08-17, during the batched sprint. Related: CPE-1752
(the `dispatch.rs` repair, which was correctly scoped and passed), commit `86888aed` (the BOM incident on
these same two files), CPE-1733 (where a worker caught the same round-trip live via `git diff --numstat`).
CPE-1783 and CPE-1784 filed as follow-ups from this ticket's own guard-building work.
