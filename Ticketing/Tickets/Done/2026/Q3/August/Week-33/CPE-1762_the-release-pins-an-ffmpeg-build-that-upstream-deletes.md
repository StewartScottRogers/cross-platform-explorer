---
id: CPE-1762
title: The sidecar release pins an ffmpeg build upstream deletes, and the failure reads as a corrupt archive
type: bug
priority: High
status: Done
tags: ready
estimate: S
created: 2026-08-15
closed: 2026-08-17
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

## Work Log (2026-08-17, worker on branch CPE-1762-ffmpeg-pin-fails-honestly)

Picked this up expecting to do the re-pin + fail-honest work from scratch, but found it had already
landed directly on `main` in commit `86888aed` ("fix: strip the BOM I added to the manifests, and unrot
the ffmpeg pin", 2026-08-15) as part of an out-of-band hotfix to unblock release 0.57.66. That commit
already re-pinned `FFMPEG_BUILD_TAG`/`FFMPEG_BUILD_VER` to `autobuild-2026-08-15-13-02` /
`n8.1.2-44-g7c533d0f86`, added the `fetch()` helper with an HTTP-status check, and routed every
pdfium/ffmpeg download in `.github/workflows/release-sidecar.yml` through it (this ticket's own file
was even committed alongside it, still in `Backlog/`).

Per instructions not to trust a two-day-old pin (BtbN prunes autobuilds daily), re-verified before
touching anything, from this worktree, on 2026-08-17:

- `https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-15-13-02/ffmpeg-n8.1.2-44-g7c533d0f86-win64-lgpl-8.1.zip` → **HTTP 200**
- `https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-15-13-02/ffmpeg-n8.1.2-44-g7c533d0f86-linux64-lgpl-8.1.tar.xz` → **HTTP 200**
- `https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F7961/pdfium-win-x64.tgz` → **HTTP 200**
- Sanity check on the original, already-dead pin from the ticket body — `.../autobuild-2026-08-01-13-21/ffmpeg-n8.1.2-34-g9b6c8969e0-linux64-lgpl-8.1.tar.xz` → **HTTP 404**, confirming the rot the ticket describes and that the current pin has not (yet) rotted the same way.

Since the current pin is still live, did **not** re-pin again — that would just be churn. What was
actually missing against the acceptance criteria:

- Criterion "the same guard covers the pdfium download" — already true in the landed commit (`fetch`
  wraps both pdfium and ffmpeg calls on every OS arm).
- Criterion "a deliberately wrong pin fails with an error naming the URL and HTTP status, not a tar/xz
  error" — extracted the `fetch()` function verbatim into a standalone script and ran it against the
  known-dead 2026-08-01 pin. Real output:
  ```
  ::error::download failed (HTTP 404): https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-01-13-21/ffmpeg-n8.1.2-34-g9b6c8969e0-linux64-lgpl-8.1.tar.xz
  ::error::If this is a BtbN ffmpeg URL, the pinned autobuild has been deleted upstream -
  ::error::bump FFMPEG_BUILD_TAG/FFMPEG_BUILD_VER to a current release. See CPE-1762.
  ```
  exit code 1, no tar/xz invoked. Confirms this criterion.
- Criterion "whatever long-term approach is chosen is recorded at the workflow with the reasoning" —
  this was **not** satisfied by the landed commit: the reasoning existed only in that commit's message,
  not in the YAML itself. Added a comment block in `release-sidecar.yml` (decide-and-log, no question
  asked per sprint rules) recording the three options considered — self-mirror, build-from-source on
  Windows/Linux like the macOS leg, or a scheduled weekly freshness-check job — and the call: recommend
  the scheduled check as a follow-up ticket (not built here, per the ticket's own instruction), because
  mirroring adds a signing/maintenance burden and building from source would multiply Windows/Linux
  build time for a dependency that rarely changes. Also fixed a stale reference in the licensing
  comment block above the step, which still named the dead `autobuild-2026-08-01-13-21` tag instead of
  pointing at the `FFMPEG_BUILD_TAG` env var.
- Criterion "a release build succeeds on all three OSes with a current pin" — **not directly
  verifiable from this worktree**; triggering `release-sidecar.yml` requires a tag push, which is a
  Foreman/release-owner action, not something a ticket worker does. Verified everything short of an
  actual run: both live asset URLs return 200, the YAML parses (`python -c "import yaml; ...load(...)"`
  → OK), and the failure branch produces the exact honest-failure output above. Flagging for the
  Foreman/next release cut to confirm the actual green run.

No re-pin churn, no scope creep beyond the acceptance criteria. Diff is comment-only plus the missing
long-term-reasoning block — 15 insertions / 3 deletions in `.github/workflows/release-sidecar.yml`.

## Work Log addendum (2026-08-17, UAT fix round)

UAT on the PR caught a real defect squarely inside this ticket's own acceptance criterion: on a DNS
failure or refused connection, the guard printed `HTTP 000000` — two `000`s concatenated — instead of a
plausible status. Root cause at `.github/workflows/release-sidecar.yml` (the `fetch()` helper): `curl
--write-out '%{http_code}'` already writes `000` to stdout on a connection-level failure, and the
`|| echo 000` fallback then appended a *second* `000` into the same command substitution, since curl's
own exit code was nonzero even though it had already printed a value. `$code` ended up as the
concatenation `000000`.

Fixed by separating "did curl fail" from "what did curl report":

```sh
code=$(curl -sSL --write-out '%{http_code}' -o "$out" "$url") || true
code="${code:-000}"
```

`|| true` stops `set -euo pipefail` from aborting on curl's nonzero exit while still capturing whatever
curl already printed; `${code:-000}` only substitutes `000` if curl printed *nothing at all* (a case
that shouldn't happen with `--write-out` but is a safe default). No more double-000 concatenation.

Extracted the patched `fetch()` verbatim (same `code=$(...)`/conditional lines) into a standalone
script and ran it against four cases:

- Known-dead pin (`autobuild-2026-08-01-13-21`, linux64 asset) → `::error::download failed (HTTP 404): <url>` — as before, unaffected by this fix.
- `https://this-host-does-not-exist-cpe1762.invalid/x.tar.xz` (DNS failure, curl error 6) → `::error::download failed (HTTP 000): <url>` — exactly three zeros, confirmed.
- `http://127.0.0.1:1/x.tar.xz` (connection refused, curl error 7) → `::error::download failed (HTTP 000): <url>` — exactly three zeros, confirmed.
- Live URL (`autobuild-2026-08-15-13-02` win64 zip, still HTTP 200 as of this check) → succeeds, exit 0 — confirms the guard is still a discriminator and doesn't fail closed on everything.

All four real outputs captured above verbatim from the terminal.

Re-confirmed `python -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release-sidecar.yml'))"` → OK after the fix.

Deliberately did **not** add a content-type/magic-byte check for a 200-with-HTML-body — Foreman flagged
that as a real residual risk but explicitly out of scope for this ticket and filed separately. Stayed
in scope: this addendum only touches the `code=$(...)` line and its immediate default.
