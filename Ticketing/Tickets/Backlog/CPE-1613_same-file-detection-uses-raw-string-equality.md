---
id: CPE-1613
title: "Batch media decides \"is this the same file?\" by raw string equality — a JPG→jpg convert overwrites the original on Windows"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Found by the independent reviewer on CPE-1599 (PR #812) while probing the new engine-side overwrite guard.
**Not a regression from that PR** — the flaw is in the definition of "same file" that both the new guard and
the *pre-existing* non-destructive guarantee have always shared. It is filed High because the worked example
below is an ordinary thing to do on the user's primary platform, and it destroys an original in the mode
that promises not to.

## The bug
`any_in_place` (`crates/server/src/batch_execute.rs:129`) and `overwritesInPlace`
(`src/lib/batchMedia.ts:126`) both decide whether an operation overwrites its input with **raw string
equality**: `it.input == it.output`.

Worked example, entirely mundane:
- Input `IMG_1.JPG`, operation **Convert → jpg**, "write to new files" **off**.
- `plan()` lower-cases the extension (`batch_media.rs:176`), so `output = "IMG_1.jpg"`.
- `"IMG_1.JPG" != "IMG_1.jpg"` → the guard does **not** fire, no confirmation is required, and the engine
  does not refuse.
- On Windows and default macOS the filesystem is **case-insensitive**: that write lands on the same file.
  The original is gone, with no confirmation and no checkpoint.

The same string comparison also misses: symlinks and junctions pointing at one underlying file, trailing
separators, `.`/`..` segments, 8.3 short names, and Unicode normalisation differences (NFC vs NFD, which
macOS produces routinely).

## Why it matters more than it looks
`plan()`'s **non-destructive** mode has always used the identical comparison to keep its "output != input"
promise (the `used` collision set at `batch_media.rs:154-230`). So the gap is not only in the new guard —
**the safe mode itself can silently overwrite an original** on a case-insensitive filesystem. A user who
never unticks the box, and therefore never sees a confirmation, can still lose the file.

## Fix
Canonicalize path comparison **once**, in one shared helper, and use it for both:
1. `plan()`'s non-destructive "output must differ from input" guarantee, and
2. the `confirmed_overwrite` refusal check.

They must share a single definition of "same file" — fixing one and not the other just moves the hole.
Consider what canonicalization is right per platform: at minimum case-folding on case-insensitive
filesystems and normalising separators/`.`/`..`; ideally resolving to the same underlying file
(`std::fs::canonicalize`, or comparing file identity where the OS exposes it) so symlinks and junctions are
caught too. Beware that canonicalize fails for a path that doesn't exist yet — the output usually won't —
so canonicalize the parent and compare the final component appropriately.

## Acceptance criteria
- `IMG_1.JPG` + Convert→jpg with "write to new files" **on** produces a genuinely different file, or refuses.
- The same with the box **off** requires confirmation (the guard fires) rather than silently overwriting.
- Symlink/junction, trailing-separator, `.`/`..` and case variants of the same file are all treated as the
  same file by both call sites.
- Tests cover each of those on Windows; note in the work log which cases can't be exercised on the CI matrix.

## Notes
Conflict surface: `crates/server/src/batch_media.rs`, `batch_execute.rs`, `src/lib/batchMedia.ts` and their
tests. Related: [[CPE-1599]], [[CPE-1590]]. Model: sonnet.
