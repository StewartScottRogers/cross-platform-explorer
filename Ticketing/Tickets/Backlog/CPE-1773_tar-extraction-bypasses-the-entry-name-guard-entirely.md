---
id: CPE-1773
title: Tar extraction bypasses the entry-name guard entirely, so a tar entry still vanishes into an NTFS stream
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-17
closed:
---

## Problem

Found **independently by both the Security Auditor and the UAT tester** on PR #926 (CPE-1758), each
measuring it separately and reaching the same conclusion. Two independent legs converging is the strongest
signal this crew produces.

CPE-1758 made `archive::entry_name_is_safe` refuse the shapes that let an entry's bytes land somewhere the
user cannot see. It is correct, per-segment, and it runs at every sink it was pointed at. **It is never
called on the tar path.**

`extract_tar_stream` (`crates/server/src/archive.rs:1765-1803`) relies solely on the `tar` crate's
`Entry::unpack_in`, which guards **traversal** but not colons, reserved device names, or trailing dot/space.
That function is what `start_archive_extract` — the real right-click → Extract button — uses for
`.tar`, `.tar.gz`, and `.tgz`. The one-shot `tar_unpack` (`archive.rs:1148`) has the same gap.

Measured on the CPE-1758 branch, through the real streamed extraction path:

| Entry name (TAR) | Result on the branch |
|---|---|
| `file:stream` | `Ok(done:2, errors:[])` — host file `file` is 0 bytes, the hidden stream holds 24 bytes |
| `ok/file:stream` | same silent ADS write |
| `..evil`, `con` | written as literal files |
| `nul` | aborts the whole extraction with a hard `Err` |

`errors: []`. **Zero signal.** This is the exact defect CPE-1758 was filed to close, still live today, for a
whole archive family, on the path the user actually uses.

## Why this was not a CPE-1758 blocker

CPE-1758's scope came from CPE-1733's authoritative sink table, which lists `entry_name_is_safe` as the
guard for rows 15/16 (zip) and 19/20 (7z) — never for tar. That PR did exactly what its ticket asked and is
a strict improvement: it also converts main's hard abort on `nul` into a graceful per-entry skip that lets
the rest of the archive finish. Blocking it would have left zip unfixed too.

What it must not do is imply broader coverage than it has, which is being corrected in that PR.

## What to do

- Route the tar sinks through the same guard. Note the shape difference: `unpack_in` owns the write, so
  there is no `File::create` to intercept — the entry name has to be checked before `unpack_in` is called,
  and a refused entry skipped, matching the zip path's behaviour exactly.
- Record the skip the same way the zip path does (`"{name}: unsafe entry name, skipped"` into
  `ArchiveReport::errors`) so the two formats behave identically. Divergent behaviour between formats is
  its own bug — the user does not think in sinks.
- **Then update CPE-1733's sink table**, which is the artefact that made this gap invisible. A table that
  omits a sink is how the sink stays unguarded; correcting the code without correcting the table leaves the
  next person with the same blind spot.
- Check the remaining unguarded sink while you are here: the plain-ZIP fallback `archive.extract(dest_path)`
  (`archive.rs:1345`) in the one-shot `extract_archive`. It has no live frontend caller today, but the
  command is registered (`src-tauri/src/lib.rs:8290`) and "no caller today" is not a guard.

## Acceptance criteria

- [ ] A `.tar`, a `.tar.gz`, and a `.tgz` each containing `file:stream` extract with that entry **refused**,
      no ADS written, and the skip recorded in `ArchiveReport::errors`.
- [ ] `..evil`, `con`, `nul`, `x.`, `x ` and `ok/file:stream` are refused in tar exactly as in zip — assert
      the two formats agree, in one test, so they cannot drift apart again.
- [ ] `nul` no longer aborts the whole extraction; the rest of the archive still extracts.
- [ ] Legitimate names still extract from tar: spaces, Unicode, emoji, dots mid-name, deep nesting, and a
      name containing `%` (see CPE-1758's review — `%` names were nearly lost to an over-broad check).
- [ ] The test asserts the **harm** — no ADS on the neighbouring file, no visible file — **before**
      unwrapping the `Result`. This family fails by succeeding, so an assertion after an `unwrap` is
      unreachable exactly when it matters.
- [ ] CPE-1733's sink table and `archive.rs`'s section comment list every sink and its guard, tar included.
- [ ] The docs' coverage statement matches reality after the change.

## Notes

Found on **PR #926 / CPE-1758**, 2026-08-17, during the batched sprint, by the Security Auditor and UAT
independently. Related: CPE-1758 (the guard this extends), CPE-1733 (the sink table with the gap),
CPE-1774 (zip symlink targets), CPE-1775 (a skipped entry is invisible to the user), CPE-1709, CPE-1744.
