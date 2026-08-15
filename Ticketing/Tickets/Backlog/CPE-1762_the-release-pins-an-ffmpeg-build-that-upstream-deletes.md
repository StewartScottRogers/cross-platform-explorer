---
id: CPE-1762
title: The sidecar release pins an ffmpeg build upstream deletes, and the failure reads as a corrupt archive
type: bug
priority: High
status: Backlog
tags: ready
estimate: S
created: 2026-08-15
closed:
---

## Problem

Found by cutting release 0.57.66 on 2026-08-15: **all three OS jobs failed**, and the release could not be
made at all.

`.github/workflows/release-sidecar.yml:236-237` pins:

```yaml
FFMPEG_BUILD_TAG: "autobuild-2026-08-01-13-21"
FFMPEG_BUILD_VER: "n8.1.2-34-g9b6c8969e0"
```

That BtbN autobuild **no longer exists** — measured, the pinned asset URL returns **HTTP 404**. BtbN's
`FFmpeg-Builds` publishes a fresh `autobuild-<date>` release daily and prunes old ones, so this pin has a
shelf life of weeks. The current tag as of filing is `autobuild-2026-08-15-13-02` /
`n8.1.2-44-g7c533d0f86`, and it will rot the same way.

## What made it expensive: the failure names the wrong thing

`curl -sSL` follows the 404 to an error page, writes it to `ffmpeg.tar.xz`, and hands that to `tar`:

```
xz: (stdin): File format not recognized
tar: Child returned status 1
##[error]Process completed with exit code 2
```

Nothing in that output says "the download 404'd" or names the URL. A reader's first hypothesis is a corrupt
archive or a broken tar invocation — not an upstream deletion. Windows exits `9` from the same cause with
even less to go on.

## Fix

Two parts, and the second matters more than the first:

1. **Re-pin** to a current tag. Verified working: `autobuild-2026-08-15-13-02` with
   `n8.1.2-44-g7c533d0f86` — both `ffmpeg-<ver>-linux64-lgpl-8.1.tar.xz` and
   `ffmpeg-<ver>-win64-lgpl-8.1.zip` return HTTP 200.
2. **Make the download fail honestly.** Use `curl --fail` (or check the status) and, on failure, print the
   URL, the HTTP code, and a one-line explanation that BtbN prunes autobuilds and the pin needs bumping.
   A build that dies on a rotted pin should say so in its first error line.

Worth considering while there: pin by a source that does not rot (a GitHub release of our own mirroring the
binary, or building from the FFmpeg source tag the macOS arm already uses), or add a scheduled job that
checks the pinned URL weekly and files a ticket before a release needs it. The macOS arm builds from source
(`FFMPEG_SRC_TAG: n8.1.2`) and did **not** hit this — that asymmetry is the argument.

## Acceptance criteria

- [ ] A release build succeeds on all three OSes with a current pin.
- [ ] A **deliberately wrong** pin fails with an error naming the URL and the HTTP status, not a tar/xz
      format error. Demonstrate it.
- [ ] The same guard covers the pdfium download (`PDFIUM_TAG`), which has the identical shape and the same
      rot risk.
- [ ] Whatever long-term approach is chosen — mirror, build-from-source, or a scheduled freshness check — is
      recorded at the workflow with the reasoning.

## Notes

Blocked release 0.57.66. Related: CPE-1258 (which introduced the native-deps staging step).
