---
id: CPE-1852
title: a failure on the third manifest leaves a half-bumped tree while the message says "refusing to write"
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

`scripts/release.ps1`'s `Update-ManifestVersion` writes each manifest as it goes (`:203`). CPE-1841 added
a loud guard that refuses to write when a manifest does not carry exactly one version key — but the guard
fires **per file**, after earlier files are already on disk.

Measured by the independent Reviewer during CPE-1841:

```
package.json        -> 9.9.9   (written)
tauri.conf.json     -> 9.9.9   (written)
Cargo.toml          -> 0.1.0   (guard fired)
exit 1, "expected exactly one ... found 0. Refusing to write ..."
```

The message is true of the file it names and **reads as though nothing was written**. Two of the five
version-synchronised files are now bumped, on disk, uncommitted.

## Why Medium, not High

This is strictly **better** than what CPE-1841 replaced: the old script wrote all three and reported
success having changed nothing. The abort happens before any `git` call, so no tag and no push, and the
per-file `path: old -> new` lines do disclose what landed to a reader who looks.

But it lands squarely in the hazard CLAUDE.md already records by name — a dirty working tree after a
release operation reads as unrelated noise and gets committed by accident or discarded along with real
work. That is exactly how `package-lock.json` ended up three releases behind.

## Acceptance criteria

- [ ] Either validate all manifests **before writing any**, then write all of them; or name the
      already-written files in the failure so the message stops implying nothing changed. Prefer
      validate-all-then-write-all — it makes the operation atomic in the way the message already claims.
- [ ] The failure message must not say "refusing to write" while files have been written. Whatever wording
      results has to be true of the whole run, not of one file.
- [ ] A test stages a manifest set where the **third** file fails and asserts the first two are unchanged
      on disk. Red-proof it against the current behaviour — it must fail today.
- [ ] Check the same shape for `-BumpOnly` and for the full release path; they share the writer.
- [ ] Preserve everything CPE-1841 measured: exactly `1 1` per file on `git diff --numstat`, CRLF intact,
      trailing newline intact, no BOM added, BOM preserved where one was already present.

## Notes

Found by the independent Reviewer during CPE-1841, which correctly did not absorb it — that ticket's scope
was the unscoped version regex, and this is a separate transactional property.

Read CPE-1841's Work Log first. It carries two things worth not re-deriving: the `return , $hits` trap
(the comma operator hands the caller a one-element array wrapping the whole list, so an "exactly one match"
guard reads as satisfied regardless of the real count), and the measured byte-level round-trip that any
change here must not regress.

Related: CPE-1841 (the guard that fires), CPE-1853 (the two lockfiles this script still does not touch).
