---
id: CPE-1796
title: nothing mechanical stops the ffmpeg pin going back to a rolling daily — and a neighbouring comment now names a pin that no longer exists
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

CPE-1789 repinned ffmpeg from a rolling daily to a month-end anchor, because dailies are pruned on a
~14-day window and anchors are retained indefinitely (verified: assets on the oldest anchor,
`autobuild-2024-09-30-15-36`, ~23 months old, still return 200).

**But nothing stops the next person putting a daily back.** The whole defence is an inline comment. The
freshness check detects a pin that has *already* rotted; it has no opinion on whether a pin is a daily
or an anchor, so a daily-pinned PR merges green and silently starts a fresh 14-day clock. The rot then
returns on the same cycle that produced CPE-1789.

Both the Reviewer and the UAT of PR #944 independently reached the same conclusion: the comment is
well-placed — it sits directly above the lines being edited, where a rushed editor will actually see it
— but it is guidance, not a guard.

### Second, smaller item: a comment that is now false

`.github/workflows/ffmpeg-pin-freshness.yml` (around line 19) justifies its twice-weekly cadence by
saying *"the pin currently in release-sidecar.yml, `autobuild-2026-08-15-13-02`, sits at position 5 of
the 14 live dailies and is due to be pruned around 2026-08-29."* After CPE-1789 that names a pin that
is no longer used.

Not functionally dangerous — the workflow greps the live value from `release-sidecar.yml` at run time
rather than hardcoding it (confirmed by two independent dispatch runs reading the new pin correctly).
But it is exactly the class of stale-but-confident comment this crew has spent the day correcting
elsewhere, and the cadence argument it supports changes once the pin is an anchor rather than a daily.

## What to do

- **Add the mechanical guard.** Assert that `FFMPEG_BUILD_TAG` matches a month-end date — the pattern is
  a date whose following day is the 1st, and `ffmpeg-pin-freshness.yml` already contains exactly that
  date logic for choosing which tags to recommend, so reuse it rather than writing a second copy. A
  step in CI, or a small test alongside the other workflow guards, both work; pick whichever fits where
  such assertions already live.
- Make the failure message say *why*, not just *what*: a daily pin starts a 14-day clock and will break
  a release, so pick the newest month-end anchor. A guard that only says "pattern mismatch" invites
  someone to widen the pattern.
- Allow a deliberate override with a recorded reason, so the guard cannot become a thing people disable
  wholesale when they genuinely need a daily for a few days.
- **Update the cadence comment** in `ffmpeg-pin-freshness.yml` to describe the retention classes
  generally rather than naming a specific pin that will always drift, and revisit whether twice-weekly
  is still the right cadence now that the pin is an anchor — the original cadence was derived from a
  daily's ~14-day runway, which no longer applies.
- Also worth folding in: the auto-filed staleness issue text (~line 304) tells the reader to update the
  tag and version but does not itself say "pick a month-end anchor, not a daily". Someone acting from
  the issue alone would have to open the workflow to see that guidance. One sentence closes it.

## Notes

Filed by the Foreman from PR #944's Reviewer and UAT, 2026-08-19 — both flagged the missing guard and
the stale comment independently, and both explicitly called them non-blocking for that PR.

Related: **CPE-1789** (the repin), **CPE-1763** (the freshness check), **CPE-1795** (that check
recommends a version from the wrong release line), **CPE-1794** (its dedupe swallows a transient
failure).
