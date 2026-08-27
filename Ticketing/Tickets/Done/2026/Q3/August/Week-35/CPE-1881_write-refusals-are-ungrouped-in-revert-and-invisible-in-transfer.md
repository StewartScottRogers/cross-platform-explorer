---
id: CPE-1881
title: hard-link write refusals are ungrouped in revert (8 MiB of prose) and invisible in transfer (stderr only)
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-23
closed: 2026-08-27
---

## Problem

CPE-1857 added a refusal when a write would land on a multiply-linked destination. The refusal is
correct. **How it is reported is not**, in two places, found and measured by the independent Security
Auditor on PR #1016.

### 1. Revert: one full 420-byte sentence per refused file

Each refusal carries the whole explanation — what a hard link is, why no path check can see it, and
how to break the link. Measured on a 200-file fixture:

```
CMD revert[200 hard-linked files]: applied=0 skipped=200 elapsed=1.26s
      total skipped-message bytes = 84180 (420 bytes/entry avg)
      extrapolated for 20,000 entries = 8220 KiB
```

**~8.2 MiB of identical prose across the IPC for a 20,000-entry revert.** This is not hypothetical:
a tree under `rsync --link-dest` or Time Machine-style dedup is hard-linked wholesale by design, and
legitimate in-root dedup refuses too (verified: two names both *inside* the root, both refused, as
designed).

**CPE-1847 already solved this exact shape** — it grouped hold-backs into one `HeldBackSummary` after
measuring 185 KiB as the problem. Write refusals bypass that grouping entirely, and are 44× worse.

### 2. Transfer: the user is told nothing at all

`download_tree`'s hard-link arm is an `eprintln!` and nothing else. The user sees the delivered count
silently one lower — no `undelivered` entry, no reason, no count of skips. Verified in the auditor's
run: the line went to stderr and `n == 0` was the only signal.

The PR argues this deliberately, and the reasoning is sound as far as it goes: an `undelivered` entry
would fail the *whole* transfer, which is worse. But that is a false choice between "fail everything"
and "say nothing" — **the third option was not considered.**

## What to do

1. **Revert:** fold the repeated sentence into the existing summary-plus-count shape CPE-1847 built.
   One explanation, once, plus the count and the paths. Reuse `HeldBackSummary`'s pattern rather than
   inventing a parallel one.
2. **Transfer:** carry per-entry skips out of `download_tree` as a **counted list**, the way
   `ArchiveReport.skipped` already does, so the user gets "N entries skipped, here is why" without
   failing the transfer.
3. While in `download_tree`, note the adjacent **symlink** arm has the same stderr-only shape. Fix
   both or state why not.

## Out of scope — do not fix here

- The one-shot registered `extract_archive` discards its `ArchiveReport` and returns `Ok(dest)`. That
  is pre-existing for *every* guard in that command and the GUI does not use it (the frontend uses
  `start_archive_extract`, which reports correctly). Recorded so it is not mistaken for coverage.

## Acceptance criteria

- [x] A 200-file all-refused revert produces one explanation, not 200 — measured, with the before and
      after byte counts.
- [x] A transfer that skips entries reports the count and reason to the user, without failing the
      transfer.
- [x] The existing per-entry path information is not lost in the grouping.

## Work Log

- **2026-08-23 17:00 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from the Security Auditor's findings F3 and F4 on PR #1016. Both were measured rather than asserted.
  Neither blocks that PR: the refusal itself is correct and the alternative is the write going through.
- **2026-08-27 USMST** — Worked end to end on branch `cpe-1881-refusal-reporting`.
  - **Revert (item 1):** `revert_engine.rs`'s `execute_restore` now groups every hard-link write refusal
    into one `WriteRefusalGroup` (mirroring `HeldBack`'s "one explanation, not N copies" pattern) —
    `RestoreReport.write_refusal: Option<WriteRefusalGroup> { reason, count }`. Each refused path still
    gets its own (now short) entry in `skipped` — `Refused::hard_linked` carries only
    `"this file has {N} names (it is hard-linked)"`, not the ~420-byte essay — so no per-path visibility
    is lost, only the repeated boilerplate. The shared paragraph is built from the SAME
    `LinkGuardWording::hard_link_reason()` fields `fsutil::copy_file_onto_no_follow_with_wording` uses
    for its own single-entry message, so the two forms cannot drift apart. Wired through
    `checkpoint_store::RevertOutcome.write_refusal: Option<WriteRefusalSummary>` (new specta type,
    `bindings.gen.ts` regenerated) and rendered in `RevertOutcomePanel.svelte` as a new block styled
    like the existing held-back paragraph. Measured on a 200-file all-hard-linked fixture
    (`revert_engine.rs::cpe_1881_two_hundred_hard_linked_refusals_cost_one_paragraph_not_two_hundred`):
    total report bytes fall from the ticket's measured 84,180 to under 20,000 (one ~400-byte paragraph
    + 200 short per-path facts), asserted directly in the test. Proved red (grouping disabled via a
    temporary `if false &&` toggle, restored after) before green.
  - **Transfer (items 2+3):** `transfer::download_tree` now returns `DownloadReport { files, skipped:
    Vec<String> }` instead of a bare `usize` — the `ArchiveReport.skipped` shape the ticket asked for,
    uncapped (never truncated). Both the hard-link leaf arm AND its adjacent pre-existing-symlink arm
    (item 3) now push a named reason into `skipped` in addition to their existing `eprintln!`, and the
    transfer still ends `Ok` — the "third option" the ticket named. Updated the one downstream caller
    (`cpe-sftp`'s `download_tree` wrapper) and every test call site across `transfer.rs`/`sftp`/`webdav`
    that read the old `usize`. `download_tree` has no live Tauri-command caller yet (traced — confirmed
    dead outside tests), so this is a backend-correctness fix with no GUI surface to screenshot.
  - **Docs:** `src/docs/16-checkpoints.md` (the hard-link bullet now says "refused" not "held back",
    since it lands in `failures`/`write_refusal`, not `held_back`, and notes the group-once behavior)
    and `src/docs/31-network.md` (new "A pre-existing link at the destination name is skipped, and now
    reported" subsection).
  - Full `cargo test` for `cpe-server`/`cpe-sftp`/`cpe-webdav` green (2397 + 36 + 32 tests). `cargo
    clippy --all-targets -- -D warnings` clean in both feature modes (default, `specta`) for all three
    crates. `npm run check` and the full `vitest` suite green (only 2 pre-existing, unrelated
    `msrvSync.test.ts` failures from this worktree's CRLF checkout of `ci.yml` — not touched by this
    ticket). `bidiEscape.guard.test.ts` caught the new raw `{summary.writeRefusalReason}` render;
    wrapped in `displaySafeName` defensively (the text is backend-composed from a count + static
    wording today, never a raw path, but the wrap costs nothing).
  - Opened PR #1046 against `main`, branch `cpe-1881-refusal-reporting`. CI still pending as of this
    entry. Moving to Done per the sprint's ticket-flow convention (folder location tracks work state,
    not merge state); the PR is the durable record if CI turns something up.
