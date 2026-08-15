---
id: CPE-1755
title: A save to a free name lands 0600, and two doc lines describe the mode carry inaccurately
type: task
priority: Low
status: Backlog
tags: ready
estimate: XS
created: 2026-08-15
closed:
---

## Problem

Observed by the PR #913 (CPE-1739) round-2 reviewer with `strace`, and explicitly judged non-blocking —
"worth folding into the next touch of this function".

CPE-1739 made the staging file be **created** at `0600` (closing a window where a private file was briefly
world-*openable* while being staged), then `fchmod`s it to the target's real mode. That fix is correct. Two
loose ends came with it:

### 1. A save to a brand-new name now lands `0600`

```
openat(AT_FDCWD, ".../brand-new.json.6653-....cpe-tmp", O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC, 0600) = 3
rename(".../brand-new.json.6653-....cpe-tmp", ".../brand-new.json")
```

No `fchmod` — `existing` is `None`, so `carry_protections` never runs and nothing widens it. Previously
such a file landed at the umask default (typically `0644`).

**Unreachable from production today**: `metadata_write_impl` reads the file before writing, so the target
always exists, and `write_file_text`'s Save-As does not use this function. It also errs on the **safe**
side. But the free-name path is deliberately supported and has its own test, so the behaviour should be
either intended-and-documented or changed.

Decide which: leaving it `0600` is defensible (a new file created by a tool arguably should not be
world-readable by default), but it is a silent departure from what every other file-creating path in the
app produces. Whichever way, say so at the site.

### 2. Two doc lines are inaccurate

- `STAGING_MODE`'s doc says the `fchmod` "only ever **widens** it to whatever the user's own file actually
  had". Not true for a `0400` target — there it **narrows**.
- The same doc does not cover the case where there is no user file at all (item 1 above).

## Acceptance criteria

- [ ] The brand-new-file mode is a recorded decision — either documented as intended at
      `create_staging_file`/`STAGING_MODE`, or changed to match the platform default, with the reason
      written down.
- [ ] `STAGING_MODE`'s doc describes what the `fchmod` actually does in all three cases: widen (`0644`
      target), narrow (`0400` target), and no target at all.
- [ ] No behaviour change to the carry itself — CPE-1739's narrow-then-widen ordering, its umask-controlled
      staging-mode test, and the `strace`-verified absence of a `0666` creation must all still hold. If item
      1 is changed rather than documented, the staging file must still be **created** at `0600`; only the
      final mode of a brand-new file may differ.

## Notes

Related: CPE-1739 (PR #913). The reviewer's round-1 blocker on that PR was the disclosure window this
`0600`-at-creation change closed; do not undo it while addressing item 1.
