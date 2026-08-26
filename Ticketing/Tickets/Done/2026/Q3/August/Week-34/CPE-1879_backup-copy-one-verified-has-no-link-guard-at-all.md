---
id: CPE-1879
title: backup.rs::copy_one_verified writes to a user-named destination with no link guard at all — not even for symlinks
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-23
closed: 2026-08-25
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

- [x] The write-through reproduced on today's code and shown closed — both pasted.
- [x] Symlink/reparse refusal present, matching the sibling write paths.
- [x] The hard-link decision recorded with its reasoning, whichever way it goes.
- [x] The directory-nlink asymmetry handled and pinned on all three OSes.
- [x] Any refusal is reported to the user, per file — never a silent skip.

## Work Log

- **2026-08-23 16:50 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from CPE-1857's worker's own sweep. It fixed the three untrusted-name sites in its scope, gave
  reasons for the sites it deliberately left alone, and flagged this one as out of scope rather than
  quietly widening its diff. That is the right call and it is why this ticket exists.
- **2026-08-23 (Worker)** — Fixed. `copy_one_verified` now calls
  `fsutil::copy_file_onto_no_follow` (CPE-1857's mechanism) instead of a bare `std::fs::copy`, so it
  inherits the symlink/reparse refusal and the `!is_dir && links > 1` hard-link refusal at zero extra
  syscalls, read off the same open handle the write goes through.
  - **Reproduced live before the fix**: an out-of-tree `outside/victim.txt` hard-linked to the backup
    destination `dst/h.txt`, then `copy_one_verified` run against it — `victim.txt`'s bytes changed to
    the backup source's content (`cargo test` panic on the HARM assertion, pasted in the PR/report).
    Same reproduction for a symlinked destination. A third test proved the silent-write shape at the
    `apply_backup_plan` level: the OpResult for the linked entry came back `ok: true`, no error, no
    signal anything was wrong. All three now pass after the fix (refused, per-file, victim untouched).
  - **Hard-link decision: refuse, same as CPE-1857's restore writer** — deliberately re-derived, not
    copied across. This backup engine implements no dedup of its own: `copy_one_verified` only ever
    executes a flat copy/update plan computed by comparing two trees; it never decides "link instead of
    copy". A real dedup backup tool (rsync `--link-dest`, Time Machine) creates its *own* hard links as
    a deliberate step and would never subsequently `fs::copy` onto one. So a pre-existing multiply-linked
    name at a plan-chosen destination is not this tool's own structure — it is either an accident (backup
    pointed at a store some other tool manages) or a planted link — and writing through it is corruption
    either way. Refusing, per entry, with the rest of the run continuing (already how
    `apply_backup_plan_walk` treats every `copy_one_verified` error) transfers CPE-1857's answer for the
    same reason it applied there.
  - Symlink refusal added unconditionally — no legitimate counter-case for a backup destination being a
    link.
  - Reporting: no plumbing change needed — `apply_backup_plan_walk` already converts any
    `copy_one_verified` error into `OpResult::err(&dst, e)` and continues the loop, so the refusal was
    already wired to reach the caller per file; only the refusal itself was missing. Pinned by
    `apply_backup_plan_reports_a_hard_link_refusal_per_file_not_silently`.
  - Directory/nlink asymmetry: inherited from `copy_file_onto_no_follow`, which orders the `is_dir`
    refusal ahead of the `links > 1` check, so a directory can never reach the hard-link branch on any
    platform — no new asymmetry introduced.
  - `cargo test` (crates/server): 2383 passed, 0 failed, 8 ignored. `cargo clippy --all-targets` and
    `cargo clippy --all-targets --features index`, both `-D warnings`: clean.
  - New doc bullet in `src/docs/safety-undo.md` explains the refusal in plain language alongside the
    existing Windows-odd-name refusal bullet.
  - Security-relevant: flagging for the Security Auditor leg.
- **2026-08-24 (Worker) — PR #1022 review round 1: Reviewer APPROVE, Security Auditor SEC FINDINGS,
  three blocking findings, all addressed.** Auditor independently reproduced both refusals through the
  public `apply_backup_plan` on the head commit (hard link: refused, victim intact; symlink: same),
  confirmed the wide TOCTOU window is not exploitable (the guard reads the handle, not a path, so a link
  planted between plan-computation and execution is still caught), confirmed no zero-byte debris on
  refusal (`set_len(0)` runs strictly after every refusal), and structurally verified this call site
  inherits CPE-1857's *fixed* shape (`handle_facts` has no `is_degenerate` filter, unlike the contrasting
  site at `fsutil.rs:1789`, so a NAS reporting a zero file index still fires the guard).
  - **Finding 1 (blocked on the claim, not the code) — the parent-directory route.**
    `create_dir_all(parent)` walks through a directory junction with no guard at all, identical on `main`
    and this branch; a junction needs no privilege on Windows (unlike the symlink/hard-link legs), and a
    backup destination is typically the least-defended directory on the box. Not fixed here — the Foreman
    is filing it as its own ticket (parent-directory containment on the write side), and burying that
    inside a link-guard PR was the wrong shape. What changed: `copy_one_verified`'s doc comment now states
    the guard is scoped to the **final component only**, names the junction bypass explicitly, and notes
    the write/delete asymmetry (`apply_backup_plan_walk`'s delete loop already asserts `contained_under`
    on the resolved path; nothing equivalent runs before a write). `src/docs/safety-undo.md` updated to
    match — "the last step of the path only", plus a new bullet naming the gap in plain language.
  - **Finding 2 (blocked on the claim; the behaviour is real and tracked separately) — Windows ADS /
    `Zone.Identifier` loss.** `fs::copy` (`CopyFileExW`) carries alternate data streams; this function's
    open → `set_len(0)` → byte-stream path carries none, and a stale `Zone.Identifier` on an overwritten
    destination survives instead of being cleared. Measured by the Auditor: `main` preserves the mark,
    this branch drops it, and a stale mark on the destination survives an overwrite it shouldn't. Not
    fixed here (Foreman filing separately). What changed: `copy_one_verified`'s doc comment now states
    this explicitly, notes that `copy_file_onto_no_follow`'s own accepted-cost reasoning ("the user's own
    captured content", "toward keeping a warning") does **not** hold at this call site (arbitrary source
    tree, direction is toward *dropping* a warning), and records — unmeasured — that macOS's
    `fcopyfile(COPYFILE_ALL)` probably carries `com.apple.quarantine` the same way, without asserting it.
    `src/docs/safety-undo.md` states the gap in plain language for users.
  - **Finding 3 (blocked; fixed) — the refusal reached `apply_backup_plan_walk`'s caller but never a
    screen.** `OpResult.error` was computed correctly but never rendered: `BackupDashboard.svelte` only
    ever showed `{ok} ok, {failed} failed`, and the auto-run notice (`App.svelte::runBackupJobNow`) only
    ever showed the same two counts. A dedup-store-backed job would refuse every entry, every run,
    forever, with no reason on screen and a remedy telling the user to break their own store. Fixed:
    - `RunStatus`/`BackupRunRecord` gained an optional `firstError: { path, error }`, populated from the
      first `!ok` entry in the streamed results, in both `BackupDashboard.svelte::apply()` and
      `App.svelte::runBackupJobNow`.
    - `BackupDashboard.svelte` renders it: a `.status-detail` line under the live/last-run status pill
      (`data-testid="job-status-detail"`) and inline in each history row (`.hist-detail`), both bidi-safe
      via `displaySafePath` (backend error text can embed a filesystem path).
    - The auto-run notice gets a second sentence via a new i18n key `notice.autoBackupFirstFailure`,
      added to English plus the other 11 `COMPLETE_LOCALES` (es/de/fr/it/pt/nl/pl/ru/zh/ja/ko) to hold the
      CPE-481 100%-coverage gate; the remaining offered locales fall back to English for it, same as any
      other key.
    - Remedy text ([`LinkGuardWording::BACKUP`] in `fsutil.rs`, replacing the shared restore-only wording
      the reviewer separately flagged) no longer universally says "break the link" — it now names the
      dedup-store case first and tells that owner to leave the refusal alone, then gives the break-the-
      link remedy for everyone else.
    - Noted per the Auditor's ask: the **Restore** direction reverses the roots, so `dst` there is the
      user's live tree — more likely to hold a pre-existing hard link (package stores, dedup sync
      clients) than a fresh backup destination is. Called out in `copy_one_verified`'s doc comment and in
      `src/docs/safety-undo.md`.
    - Three new `BackupDashboard.test.ts` cases pin: the status line shows the failed path + "hard-linked"
      reason text; the dispatched `run` event's `status.firstError` matches the streamed `OpResult`; a
      fully-successful run carries no `firstError`.
  - **Reviewer's three cheap doc findings, fixed alongside:** the shared refusal text hard-coded "a
    restore ... never writes through one" and "the folder being restored", wrong once this call site
    started using the same function — `fsutil::copy_file_onto_no_follow` now takes a
    `LinkGuardWording` (`RESTORE` default, unchanged for every existing caller/test; `BACKUP` for this
    site) so the wording is accurate per caller. The stale "only production caller is
    `revert_engine::apply_write`" comment is corrected to name all three current callers. The "zero extra
    syscalls" claim in `copy_one_verified`'s doc comment is corrected to state plainly that the *guard
    check itself* is free (reads facts off the handle already open for the write) but the function this
    replaces `fs::copy` with is not — roughly three to five syscalls per file where `fs::copy` was one,
    a cost already accepted for the restore path by CPE-1870's measurement and carried here, not
    re-measured.
  - Collateral from the App.svelte/i18n.ts edits: two line-number-pinned guard tests
    (`bidiEscape.guard.test.ts`'s `APP_MARKUP_OFFENDERS`/`APP_SCRIPT_BASENAME_ALLOWLIST` and
    `mojibakeGuard.test.ts`'s Portuguese "NÃO" allowlist entry) re-anchored to the new line numbers, plus
    a genuinely new `BackupDashboard.svelte` bidi-guard registry entry removed by wrapping the two new
    raw-error renders in `displaySafePath`.
  - Verified again after all of the above: `cargo test` (crates/server) 2383 passed/0 failed/8 ignored;
    `cargo clippy --all-targets` and `--features index`, both `-D warnings`, clean; `npx svelte-check`
    0 errors/0 warnings; full frontend `vitest run` — 331 files, 4457 tests, all green.
  - Pushed to `cpe-1879-backup-link-guard`; Foreman owns CI from here.
