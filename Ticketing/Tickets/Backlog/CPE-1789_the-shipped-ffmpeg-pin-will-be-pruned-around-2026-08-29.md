---
id: CPE-1789
title: the shipped ffmpeg pin will be pruned around 2026-08-29 — move it to a month-end anchor
type: bug
priority: High
status: Backlog
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

`release-sidecar.yml` pins `FFMPEG_BUILD_TAG: autobuild-2026-08-15-13-02`. BtbN prunes its rolling
daily autobuilds, and that tag currently sits at **position 5 of 14** in the live daily window. On the
observed cadence it will be pruned around **2026-08-29 — roughly ten days from filing.**

When it goes, the release build starts fetching a 404 and the sidecar-enabled release stops building.

## How this was measured

Two independent agents queried BtbN's release list while reviewing PR #938 (CPE-1763) and got the same
breakdown of the 37 live `autobuild-*` releases:

- **14 rolling dailies** — `autobuild-2026-08-06-13-39` through `autobuild-2026-08-19-19-21`.
- **23 month-end anchors** — one per month (`*-07-31-*`, `*-06-30-*`, …) going back to
  `autobuild-2024-09-30-15-36`, i.e. nearly two years, **retained indefinitely**.

PR #938's original design comment read the raw count of 37 as "~38 daily autobuilds, so a freshly-set
pin has roughly five weeks of runway". That conflates the two classes. The real runway for a pin set to
a **daily** build is ~14 days.

## What to do

**Repin to a month-end anchor rather than bumping to a newer daily.** The month-end releases are
retained indefinitely, which does not merely postpone this rot — it removes it.

- Candidate: `autobuild-2026-07-31-14-10`, whose Windows asset is
  `ffmpeg-n8.1.2-34-g9b6c8969e0-win64-lgpl-8.1.zip`.
- Update `FFMPEG_BUILD_TAG` **and** `FFMPEG_BUILD_VER` together — they are used to build the asset URL
  and must agree, and note the `-lgpl-<x>.<y>` suffix in the filename also tracks the ffmpeg release
  line (it moved 8.0 → 8.1 within six months), so verify the constructed URL resolves rather than
  assuming.
- Verify by HEAD-ing every asset URL the release workflow actually fetches — Windows and Linux ffmpeg —
  before merging, and record the statuses in the Work Log.
- Say in the workflow comment **why** a month-end tag is pinned, so the next person bumping it does not
  quietly move back to a daily and reintroduce the 14-day clock.

Consider whether a slightly older, permanently-retained build is acceptable versus the newest daily.
For a shipped release the answer is almost certainly yes: reproducibility and a build that still exists
in six months beat two weeks of freshness.

## Notes

Filed by the Foreman from PR #938's Reviewer and UAT, 2026-08-19, both of which independently measured
the retention shape. This is the pin itself; **CPE-1763** is the scheduled check that watches it, and
that check is deliberately not doing this repin — a monitor should not also be the thing it monitors.

Related: **CPE-1763** (the freshness check), **CPE-1787** (unhardened `apt-get` sites in `ci.yml`,
including the ffmpeg install step that was measured stalling silently for 1h36m).
