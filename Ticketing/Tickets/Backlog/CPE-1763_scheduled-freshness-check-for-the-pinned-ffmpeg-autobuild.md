---
id: CPE-1763
title: A scheduled freshness check for the pinned ffmpeg autobuild, so the pin is bumped before a release needs it
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-17
closed:
---

## Why this exists

Split out of **CPE-1762**, which fixed the symptom and recorded the reasoning but deliberately did not
build this. CPE-1762's own conclusion, recorded in `.github/workflows/release-sidecar.yml`: of the three
durable options — (a) self-mirror the binary as a release of our own, (b) build from source on
Windows/Linux the way the macOS leg already does, (c) a scheduled job that checks the pin's freshness —
**(c) is the recommended follow-up**. (a) adds a signing and maintenance burden for a binary we do not
otherwise own; (b) would multiply Windows/Linux build time for a dependency that rarely changes.

The problem it solves: BtbN's `FFmpeg-Builds` publishes a fresh `autobuild-<date>` release daily and
**prunes old ones**, so `FFMPEG_BUILD_TAG` / `FFMPEG_BUILD_VER` in `release-sidecar.yml` rot on a
timescale of weeks. Measured on 2026-08-15: the pinned asset returned **HTTP 404** and **all three OS
release jobs failed**, blocking release 0.57.66 outright. CPE-1762 re-pinned it and made the failure name
the URL and status instead of masquerading as a corrupt archive — but the *next* rot is still a release-day
surprise, just a legible one.

This ticket turns "release-day surprise" into "a ticket filed with days of runway".

## What to build

A scheduled GitHub Actions workflow (weekly is the cadence CPE-1762 reasoned to) that:

- Reads `FFMPEG_BUILD_TAG` / `FFMPEG_BUILD_VER` **from `release-sidecar.yml` itself** rather than
  duplicating the values — a freshness check with its own copy of the pin is a second thing to rot.
- Issues a HEAD request for each pinned asset actually used by a release: the win64 zip and the linux64
  tar.xz (the macOS leg builds from source and is not exposed to this).
- Also checks the pdfium pin (`bblanchon/pdfium-binaries`, currently `chromium/7961`) while it is there —
  same failure shape, same blast radius, and it costs one more request. Confirm first whether that
  publisher actually prunes; if it does not, say so and check it anyway or record why not.
- On a non-200, **files a ticket** (or opens an issue, or fails loudly in a way someone actually sees —
  decide and record which, given nobody watches a red scheduled run by default) naming the URL, the
  status, and the current live tag to bump to.

## Acceptance criteria

- [ ] A scheduled workflow exists and its schedule is stated in the file with the reason for that cadence.
- [ ] It reads the pins from `release-sidecar.yml`, with no second copy of the tag/version values
      anywhere. Breaking that (changing the pin in one place) is caught by the check itself.
- [ ] A deliberately-dead pin makes the check fail, and the failure names the URL, the HTTP status, and
      the current live tag to bump to. Demonstrate with real output against the known-dead
      `autobuild-2026-08-01-13-21`.
- [ ] A live pin passes and produces no ticket/issue/noise — a check that cries wolf weekly gets muted,
      which is the same as not having it.
- [ ] The notification path is verified end-to-end at least once, not merely written. If the mechanism is
      "the run goes red", say explicitly who sees a red scheduled run and how.
- [ ] Running it does not require secrets beyond the default `GITHUB_TOKEN`.

## Notes

Related: **CPE-1762** (the rot that blocked release 0.57.66; PR #922 recorded the reasoning this ticket
implements), CPE-1258 (introduced the native-deps staging step).

Filed by the Foreman during the batched sprint of 2026-08-17, on the reviewer-confirmed recommendation in
CPE-1762's Work Log.
