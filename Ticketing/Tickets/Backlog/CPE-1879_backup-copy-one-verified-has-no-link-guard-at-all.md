---
id: CPE-1879
title: backup.rs::copy_one_verified writes to a user-named destination with no link guard at all — not even for symlinks
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-23
closed:
---

## Problem

`backup.rs::copy_one_verified` is a bare `std::fs::copy` onto a **user-named backup destination**,
with **no link guard of any kind** — not the hard-link refusal CPE-1857 just added, and not even the
symlink / reparse-point check that the restore writer, archive extractor and transfer downloader all
already have.

Found by CPE-1857's worker while sweeping every write site in the codebase for the same defect. It
gave 54 production sites an explicit verdict — refuse, or accept-with-reason — and committed that
table alongside the fix. This is the one site it flagged as neither: **not analysed and not guarded**.

## Why it matters

`fs::copy` writes **through** an existing inode. That is the whole mechanism behind CPE-1857: if the
destination name is a hard link to a file elsewhere, the copy rewrites that other file, and no path
containment check can see it, because a hard link has no target to resolve and the path genuinely is
where it says it is.

The same is true through a **symlink**, which is cheaper to arrange and which every sibling write path
already refuses.

So a backup — an operation whose entire purpose is to not destroy data — can currently overwrite a
file the user never named, in a place the backup was never pointed at.

## Scope and honest severity

Medium, not High, and the reason is the precondition: something must already have placed a link at the
destination name. A backup destination is usually a fresh directory the user chose, so this is not a
one-click exploit. It is a missing guard on a data-destroying path where every comparable path has
one — defence that should exist, not a live incident.

## What to do

1. Read CPE-1857's committed verdict table (on `batch_media::name_is_multiply_linked`) and the
   refusal it added to the restore writer. **Reuse that mechanism** rather than inventing a second
   one — it reads the link count off the handle the code already opens, at zero extra syscalls, and
   cannot be defeated by a path swap between check and write.
2. Decide, and record, whether a backup onto a legitimately hard-linked destination should refuse or
   proceed. CPE-1857 chose refuse-per-entry-loudly, with the rest of the batch still applying; the
   dedup-backup and package-store cases it names apply here too, arguably more strongly, since a
   backup target may legitimately be a deduplicating store. **This is the real design question in the
   ticket** — do not just copy the answer across without thinking about it.
3. Add the symlink/reparse refusal regardless. That one has no legitimate counter-case here.
4. **Prove it.** Reproduce the write-through on today's code — the destination name linked to a file
   outside the backup root, then a backup — and show the outside file changing. Then show it refused.
   CPE-1857's harm tests are the template; copy their shape, including the `HARM:` assertion messages
   that say what went wrong in plain words.

## Cross-platform trap, already paid for once

CPE-1857's CI caught this and it will catch you too: **on Unix every directory has `nlink >= 2` by
construction; on Windows a directory's is 1.** A bare `links > 1` check reddens Linux and macOS while
passing on Windows. The correct form is `!is_dir && links > 1`, pinned by an all-platform test whose
failure message states the asymmetry.

## Acceptance criteria

- [ ] The write-through reproduced on today's code and shown closed — both pasted.
- [ ] Symlink/reparse refusal present, matching the sibling write paths.
- [ ] The hard-link decision recorded with its reasoning, whichever way it goes.
- [ ] The directory-nlink asymmetry handled and pinned on all three OSes.
- [ ] Any refusal is reported to the user, per file — never a silent skip.

## Work Log

- **2026-08-23 16:50 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from CPE-1857's worker's own sweep. It fixed the three untrusted-name sites in its scope, gave
  reasons for the sites it deliberately left alone, and flagged this one as out of scope rather than
  quietly widening its diff. That is the right call and it is why this ticket exists.
