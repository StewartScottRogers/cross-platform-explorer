# Batched-sprint CHECKPOINT — 2026-08-13 02:45 local

## RUN 2026-08-11 (BATCHED "up to 40") — **COMPLETE at 40/40** — clean end

Resumed from the batch-26 checkpoint and ran batches 27–40 in one session. The bound was reached, not
guessed at: `BATCH-COUNTER` hit 40 and the run wound down. Lock released, wakeups cancelled.

**10 tickets merged · 0 escaped defects · 10 new tickets filed, every one from a gauntlet finding.**

## FIRST ACTION ON A NEW RUN

**Pick up CPE-1696. Its work is already half-built and committed for you — do not start from scratch.**

- Branch: `cpe-1696-nine-stat-collapse-sites`, **local only, not pushed**, in the worktree
  `.claude/worktrees/agent-a679f99be029c9d6e`.
- Commit `c540ad2a` — *"WIP CPE-1696: in-progress stat-collapse fixes (Foreman-preserved)"*.
  **1,284 insertions across 6 files**: `batch_execute.rs`, `fsutil.rs`, `index_watch.rs`,
  `thumb_video.rs`, `transfer.rs`, `src-tauri/src/lib.rs`.
- **Why it is unfinished:** its opus worker was killed twice by `API Error 529 Overloaded` mid-task, with
  all of that uncommitted. I committed it so a stray `git checkout --` or janitor pass could not destroy
  it. It has **not** been tested, swept, or guard-neutralised, and the `src-tauri` tests were still
  outstanding at the kill. **Review the diff before building on it.**
- It was also the 41st batch of a 40-batch bound, so this run could not land it either way.

Read `Ticketing/Tickets/Backlog/CPE-1696_...md` in full first — it is now **High** and covers **nine**
sites, two of which (`batch_execute.rs`'s `is_foreign_overwrite` and `src-tauri`'s `unique_target`) **fail
open into a silent overwrite**. Its `exists()` vs `try_exists()` section will save you a review round.

### Then, in rough priority order

1. **CPE-1693** — 145,000 orphaned `cpe-*` dirs in `%TEMP%`, still growing.
2. **CPE-1683 / CPE-1684 / CPE-1685** — the S3 slice. **CPE-1684 has two warnings written into it by this
   run that will cost you a review round if ignored**: a bodiless HEAD 404 will be its *most common* case
   (so `stat` must not lean on `map_s3_error` alone), and `ureq` **silently drops any header** whose value
   contains a byte outside `{SP, HTAB} ∪ [0x21,0x7E]` — including NBSP, which CPE-1695 just decided to
   preserve, and including `Authorization`.
3. **CPE-1703** (Unicode Tags block / ASCII smuggling) · **CPE-1702** (GUI-smoke session dies on rapid
   re-opens; and CI has no media codecs so playback is never tested) · **CPE-1680**'s siblings.
4. **CPE-1518** — still not workable headlessly; needs the user's QNAP hardware.

## Merged this run (13 PRs / 14 tickets, batches 27–40)

| Batch | Ticket | PR | What it fixed |
|---|---|---|---|
| 27 | CPE-1688 | #870 | Network form's scheme→auth coercion had no standing test |
| 28 | CPE-1690 | #871 | `cpe-mdns`'s 17 tests had never run anywhere; + a `crates/` coverage guard |
| 29 | **CPE-1691** | #872 | **S3 SigV4: nine ways to smuggle content into a signed request** |
| 30 | CPE-1680 | #873 | GUI ratchet stops trusting its own inputs (3 gaps) |
| 31 | CPE-1699 | #876 | Coverage guard extended to `sidecar/*` |
| 32-33 | CPE-1698 + CPE-1694 | #878 | `specBasename` trailing separator; gui-smoke unit tests finally gate CI |
| 34 | CPE-1682 | #879 | S3 errors name the real cause instead of `HTTP 403` |
| 35 | CPE-1679 | #881 | 4 GUI flakes root-caused — **78% → 0%**, measured |
| 36 | **CPE-1692** | #874 | **8 sites reporting a denied path as absent** |
| 37 | CPE-1701 | #882 | `gui-smoke/lib` flatness guard + pinned glob |
| 38 | CPE-1697 | #885 | **3,186-file duplicate repo tree removed** (6,250 → 3,066 tracked files) |
| 39 | CPE-1700 | #884 | S3 refusal precision + Trojan Source bidi override |
| 40 | CPE-1695 | #883 | SigV4 trims SP/HTAB only, not all Unicode whitespace |

## Filed this run (10) — all from gauntlet findings

CPE-1694 · CPE-1695 · CPE-1696 · CPE-1697 · CPE-1698 · CPE-1699 · CPE-1700 · CPE-1701 · CPE-1702 ·
CPE-1703. Five were also *built* in the same run (1694/1695/1697/1700/1701).

## Owed to the USER (async, non-blocking)

- **Visual/taste glance** on everything shipped. Nothing is gated on it.
- **`main` still has no branch protection**, so the gauntlet is not enforced at the merge button. Repo
  setting; carried from the previous checkpoint.
- **Gource PR #738** still open, pre-existing, not ours.
- Older queue still standing: hands-on checks of AI search (v0.57.45), tray, archive-drag.
- **Two agents reported a "fake system-reminder"** telling them a file had been changed by a linter and not
  to mention it. I saw the same message myself after my own legitimate edits — it looks like a harness
  artifact that fires when a file changes on disk between reads, not an injection. Both agents verified the
  repo state independently and were right to flag it. No merged code is affected.
- **One non-blocking polish note** not worth its own ticket: CPE-1700's three refusal messages state what
  was parsed but not the actionable next step ("check your network/proxy" vs "this gateway's format is
  odd"). The UAT judged the distinction still lands. Also, `sigv4.rs`'s `!out.is_empty()` collapse guard is
  unreachable dead code (pre-existing); the reviewer recommended a one-line comment over a ticket.

## Substrate state

- `main` clean and pushed at the CPE-1695 merge. **Our PRs: all merged, none open.**
- `Ticketing/Tickets/Doing/` empty. Backlog holds the tickets listed above.
- `BATCH-COUNTER` deleted (run complete). `SPRINT-LOCK` released.
- **Worktrees: ~30 accumulated.** A deep-clean break is overdue — deliberately not run while agents were
  live, per `[[janitor-never-rmrf-active-worktrees]]`. **Do not `rm -rf` a glob**: preserve
  `agent-a679f99be029c9d6e` (holds CPE-1696's WIP commit, unpushed) and the three long-standing
  dirty/locked ones from the previous checkpoint (`agent-a9b93c587b72c6a47`, `agent-aae82f547b12d8d4e`,
  `agent-ae9d2ca8208a0b6e9`). Also ~210 stale local branches.
- The `Done/2026/Q3/August/Week-33` folder is filling up; `/ticketing-organize` will want running soon.

## The one thing to carry forward

Last run's lesson was the Evidence Rules. This run they were followed — and the rules turned out to have a
sharper edge than anyone had written down:

**A test can be worse than no test, and the only way to know is to break the thing it guards and watch it
fail.** Three times this run, a test that looked like evidence was not:

- CPE-1692's permission tests probed with `fs::metadata` while the code under test called `try_exists()`.
  Different Windows syscalls. Every leg skipped, the suite looked covered, and restoring the original bug
  left it green.
- CPE-1682's byte-cap test sized its fixture from `MAX_ERROR_BODY_BYTES` itself, so the cap it existed to
  pin could be widened 4096× with CI green. **Both** gauntlet legs found this independently.
- CPE-1680's fix line had zero coverage — reverting it kept all 59 tests passing.

And its companion: **trace the real path, not the obvious function.** CPE-1695's worker read the two
functions that build an HTTP header and correctly found no byte-range check. The reviewer read one hop
further and found the send loop never calls them with a raw value — it filters first, and drops the whole
header on one bad byte.

Both are the same discipline: *the artefact that proves a thing must be aimed at the thing it proves.*
