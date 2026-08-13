# Batched-sprint CHECKPOINT — 2026-08-12 20:27 local

**Why this file exists:** the sub-agent budget reached its reset line (~142 of ~150 spawns this
session). This is a **hand-off, not a stop** — the batched run continues under the same count.

## To resume

Start a **fresh Claude Code session** in this repo and say **"resume the batched sprint"**. The
resuming session reads `.claude/sprint-metrics/BATCH-COUNTER` and continues the same
`completed`/`max_batches` rather than restarting the count.

```json
{ "run_id": "batched-2026-08-11-2236", "max_batches": 40, "completed": 26 }
```

**26 of 40 batches done. 14 remain.**

## State — everything is clean

- **Working tree:** clean, on `main`, fully pushed. `origin/main` = `982b4a0a`.
- **Open PRs from this sprint: none.** #738 (gource visualization) is open but predates this run
  and is not ours.
- **Nothing in `Ticketing/Tickets/Doing/`** — no stalled work-in-progress.
- **Worktrees:** 5 left. Three are older agent worktrees that were dirty or locked and were
  **deliberately not removed** (`agent-a9b93c587b72c6a47`, `agent-aae82f547b12d8d4e`,
  `agent-ae9d2ca8208a0b6e9`). Do **not** `rm -rf` these blindly — see
  `[[janitor-never-rmrf-active-worktrees]]`; use an explicit id skip-list and never `--force`.

## What merged this session (batches 21–26)

| Batch | Ticket | PR | What it fixed |
|---|---|---|---|
| 21 | CPE-1677 | #864 | GUI-smoke gate could not see a case regress inside an already-failing spec file |
| 22 | CPE-1678 | #865 | `text_stats` called a permission-denied file "not a text file" |
| 23 | CPE-1686 | #866 | `s3` is a savable connection scheme with access-key auth |
| 24 | CPE-1681 | #867 | `cpe-s3` foundation: addressing + a hand-rolled SigV4 signer |
| 25 | CPE-1689 | #868 | Eight S3 input-validation holes, incl. four keys colliding on one object |
| 26 | CPE-1687 | #869 | `join_files` said "part N missing" about a file sitting in the folder |

Zero escaped defects. Every ticket cleared an independent Reviewer **and** an independent UAT.

## Ready queue — 12 open, 11 workable

Do **CPE-1691 before CPE-1684**, and **CPE-1689's successors before CPE-1683/1684** — the S3 slices
build on each other and two tickets are explicitly gated.

| Ticket | Est | Note |
|---|---|---|
| CPE-1682 | S | S3 errors must name the real cause |
| CPE-1683 | M | `S3Provider::list`. **Decide GCS in or out first** — its XML API does not support ListObjectsV2 the same way |
| CPE-1684 | M | Object ops. **Blocked-behind CPE-1691.** Also: test whether `ureq` rewrites the signed path |
| CPE-1685 | M | Route `s3` through `cpe_vfs::open` |
| CPE-1688 | S | Network-form coercion has no standing test |
| CPE-1690 | S | `cpe-mdns`'s 17 tests have never run in CI |
| CPE-1691 | S | `sign()` accepts CRLF in headers; region/key-id unvalidated. **Do before CPE-1684** |
| CPE-1692 | M | Six sites collapse a stat failure into "not found" — incl. SFTP answering `NoSuchFile` to a remote client |
| CPE-1693 | M | 145,207 orphaned `cpe-*` dirs in `%TEMP%`, still growing |
| CPE-1679 | M | Four media-preview GUI-smoke cases flake on unchanged code |
| CPE-1680 | S | Ratchet trusts its own inputs in three places |
| CPE-1518 | — | **Not workable headlessly** — needs the user's QNAP hardware |

## The one thing to carry forward

`Ticketing/wiki.md` gained an **"Evidence Rules"** section this session. Read it before writing any
PR body. It exists because the same failure recurred five times in six tickets: **a true observation
generalised one step past its evidence.** A sweep whose conclusion was wider than its search
(three times, by three different agents, each catching the last one's miss). A comment asserting a CI
behaviour nobody had run. A ticket that carried an intent forward without its mechanism.

The rules: break each guard on its own and paste the real failure output; verify through the channel
that will actually carry the message; and state the scope of every negative result — "I found none"
is only ever "I found none *within X*".

Recent addition: restore with `git checkout --`, not by copying a backup, or a preserved timestamp
makes cargo reuse the broken binary and "restore and confirm green" lies in both directions.
