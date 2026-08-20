---
id: CPE-1789
title: the shipped ffmpeg pin will be pruned around 2026-08-29 — move it to a month-end anchor
type: bug
priority: High
status: Done
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

## Work Log

2026-08-19 — Resolved the version-string disagreement between this ticket (`n8.1.2-34-g9b6c8969e0`)
and `ffmpeg-pin-freshness.yml`'s independent recommendation (`n7.1.5-12-g1fdbca85aa`) by querying the
live release directly: `gh api repos/BtbN/FFmpeg-Builds/releases/tags/autobuild-2026-07-31-14-10`. A
single BtbN autobuild tag ships **three parallel ffmpeg release lines** as separate asset sets — a
"master" `N-125875-*` build, an `n7.1.5-12-*` build, and an `n8.1.2-34-*` build — each with its own
win64/linux64/winarm64/etc. GPL and LGPL archives. Both version strings are real assets on that tag;
the disagreement is about *which line*, not a typo.

**The ticket's `n8.1.2-34-g9b6c8969e0` is correct; the freshness check's `n7.1.5-12-g1fdbca85aa`
recommendation is wrong for this workflow.** Two reasons: (1) the currently-shipping pin
(`n8.1.2-44-g7c533d0f86`) and the macOS from-source leg's `FFMPEG_SRC_TAG: "n8.1.2"` are both already
on the 8.1.2 line, so repinning to 7.1.5 would be a silent downgrade of the ffmpeg version actually
shipped; (2) mechanically, `release-sidecar.yml` hardcodes the asset filename's release-line suffix as
literal `-lgpl-8.1` (not parameterized alongside `FFMPEG_BUILD_TAG`/`FFMPEG_BUILD_VER`) — proved this
would 404 by constructing the freshness check's exact recommendation against that hardcoded suffix:
`curl -sSL -o /dev/null -w '%{http_code}' -I ".../autobuild-2026-07-31-14-10/ffmpeg-n7.1.5-12-g1fdbca85aa-win64-lgpl-8.1.zip"`
→ **HTTP 404** (the real 7.1.5 asset is suffixed `-lgpl-7.1`, not `-lgpl-8.1`). Applying the freshness
check's suggestion as-is would have broken the release build it exists to protect. Flagging this as a
likely bug in `ffmpeg-pin-freshness.yml`'s own version-selection logic for the Foreman/CPE-1763 owner
to look at separately — out of scope here (this ticket's surface excludes that workflow file).

2026-08-19 — HEAD-verified the exact URLs `release-sidecar.yml`'s "Stage native deps" step fetches,
built from `FFMPEG_BUILD_TAG=autobuild-2026-07-31-14-10` + `FFMPEG_BUILD_VER=n8.1.2-34-g9b6c8969e0`:
  - Windows: `.../autobuild-2026-07-31-14-10/ffmpeg-n8.1.2-34-g9b6c8969e0-win64-lgpl-8.1.zip` → **HTTP 200**
  - Linux:   `.../autobuild-2026-07-31-14-10/ffmpeg-n8.1.2-34-g9b6c8969e0-linux64-lgpl-8.1.tar.xz` → **HTTP 200**

The `-lgpl-8.1` suffix hardcoded in the workflow (lines fetching win64/linux64) needed no change since
both the outgoing pin (`autobuild-2026-08-15-13-02`, `n8.1.2-44-...`) and the new one sit on the same
8.1.2 release line — but per the ticket's warning this was verified against the real asset list, not
assumed, since BtbN has moved that suffix (8.0 → 8.1) within the last six months.

2026-08-19 — Repinned `release-sidecar.yml`: `FFMPEG_BUILD_TAG` → `autobuild-2026-07-31-14-10`,
`FFMPEG_BUILD_VER` → `n8.1.2-34-g9b6c8969e0`. Added an inline comment at the pin recording *why* a
month-end anchor is used (BtbN retains one per month indefinitely vs. ~14-day pruning on dailies) so a
future bump doesn't quietly revert to the newest daily and restart the 14-day clock.

**Freshness-vs-permanence judgment (recorded per the ticket's ask):** deliberately chose an
already-19-day-old month-end anchor over the current-at-filing daily. For a shipped, signed release
artifact, reproducibility and a dependency that is still fetchable in six months are worth far more
than freshness measured in days — a daily's ~14-day shelf life is a recurring operational cost (this
exact ticket) for no benefit the release actually needs; ffmpeg patch/security freshness is not
meaningfully worse in a build from 19 days earlier. Going forward, `FFMPEG_BUILD_TAG` should only ever
be moved between month-end anchors, never to a daily, even when a daily is more "current."

2026-08-19 — Ran `gh workflow run ffmpeg-pin-freshness.yml --ref CPE-1789-repin-ffmpeg-to-month-end-anchor`
against this branch as live proof the new pin resolves: run
https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/32328780548 (id
`32328780548`), dispatched without `override_ffmpeg_build_tag` so it exercises the real path. Per
sprint contract this Worker does not poll/watch CI to completion — the Foreman verifies the run
outcome (expected: green and silent).
