---
id: CPE-1795
title: the freshness check recommends a version from the wrong ffmpeg release line — following it 404s
type: bug
priority: High
status: Done
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

`.github/workflows/ffmpeg-pin-freshness.yml` exists to tell a human which pin to bump to when the
current one rots. **Its recommendation is wrong, and following it breaks the build it protects.**

A single BtbN autobuild tag ships **three parallel ffmpeg release lines** as separate asset sets. For
`autobuild-2026-07-31-14-10` those are:

- `N-125875-*` (master snapshot)
- `n7.1.5-12-g1fdbca85aa`
- `n8.1.2-34-g9b6c8969e0`

The check picks the version by grepping the release's asset names for the first
`-linux64-lgpl-*.tar.xz` match, which lands on **`n7.1.5-12-g1fdbca85aa`**. But `release-sidecar.yml`
hardcodes the asset suffix as the literal `-lgpl-8.1`, so the URL the recommendation builds is:

```
ffmpeg-n7.1.5-12-g1fdbca85aa-win64-lgpl-8.1.zip   ->  HTTP 404
```

The real 7.1.5 asset is suffixed `-lgpl-7.1`. So a maintainer who does exactly what the issue tells
them to do repins the release workflow to a URL that does not exist.

Both version strings are genuine assets — the disagreement is about **which release line**, not a typo,
which is what makes it easy to miss.

## Why this matters more than an ordinary bug

This check is a monitor. A monitor that stays silent when it should speak is bad; a monitor that speaks
**confidently and wrongly** is worse, because it converts a person's correct instinct ("bump the pin")
into an outage. And it fires precisely when someone is under time pressure, in the middle of a release
they cannot ship.

It is also the second defect found in this workflow within hours of merging (see CPE-1792, where the
stale path died on an apostrophe before it could report at all). Both were on paths nobody had ever
executed. That is a pattern about *this* workflow, not about the people who wrote it.

## What to do

- Select the version **by the release line already in use**, not by "the first asset that matches".
  `release-sidecar.yml` pins `n8.1.2-…` and its macOS from-source build pins `FFMPEG_SRC_TAG: "n8.1.2"`,
  so the recommendation must stay on the 8.1 line unless a human is deliberately moving lines.
- Better: stop treating the version and the suffix as independent. The suffix (`-lgpl-8.1`) and the
  version (`n8.1.2-…`) are two views of the same release line and must be chosen together — derive both
  from the same chosen asset name rather than composing one from the release and the other from a
  hardcoded literal. Note CPE-1763's review already removed one hardcoded suffix from the *check*; this
  is the same class in `release-sidecar.yml`.
- **Validate the recommendation before publishing it.** The check already HEAD-checks the *current* pin;
  it should HEAD-check the URL it is about to recommend and refuse to recommend one that 404s. That
  single step would have caught this, and would catch the next variant of it.
- Red-proof: dispatch with an override so the stale path fires, and show the recommended URL resolving
  200 — where today's recommendation gives 404. The workflow is dispatchable against a branch
  (`gh workflow run … --ref <branch>`), so this is provable before merge; see CPE-1792's Work Log.

## Notes

Found by the CPE-1789 worker while repinning, 2026-08-19. It was asked to reconcile a disagreement
between the ticket's candidate version and the freshness check's recommendation, and established from
live release data that the ticket was right and the check was wrong — then verified mechanically by
constructing both URLs and HEAD-ing them (200 vs 404).

Had it simply trusted the automated recommendation over the human-written ticket, it would have shipped
a broken pin. Worth remembering the next time a check and a ticket disagree.

Related: **CPE-1763** (the check), **CPE-1792** (its stale path died on an apostrophe), **CPE-1789** (the
repin that surfaced this), **CPE-1794** (its dedupe swallows a transient failure).
