---
id: CPE-1764
title: The release download guard accepts a 200 response whose body is not an archive, so a wrong body still reaches tar
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-17
closed:
---

## Problem

Found by **CPE-1762's UAT**, which was explicitly trying to break the new guard rather than confirm it.
Recorded here because it is a real residual risk and outside CPE-1762's acceptance criteria, so folding it
into that PR would have been scope creep.

`fetch()` in `.github/workflows/release-sidecar.yml` (~line 248) checks **only the HTTP status code**:

```bash
code=$(curl -sSL --write-out '%{http_code}' -o "$out" "$url")
if [ "$code" != "200" ]; then ... exit 1; fi
```

Measured: fetching `https://httpbin.org/html` — an HTML page served with **HTTP 200** — was **accepted**,
exit 0, and the HTML document was written to the output path as if it were an archive. It would then be
handed to `tar`/`unzip`, reproducing exactly the misleading `xz: File format not recognized` error that
CPE-1762 exists to eliminate.

## Why it is Low, and why it is still worth a ticket

BtbN's actual rot is **404-shaped** — they delete the release, so the pinned asset genuinely 404s, and the
status check catches that. This is the failure mode that has actually bitten us, twice, and it is closed.

The gap is the *other* shapes: a CDN or proxy serving an error page with a 200, a repository that replaces
a pruned asset with a placeholder, an interception page on a restricted runner network, or a partial body
truncated by a dropped connection but still reported 200. None has happened here. But the guard's whole
justification is "the first error line should tell the truth", and in these shapes it still lies in exactly
the old way.

Note the shape of this defect: **it fails by succeeding.** That is the class this repo has been bitten by
repeatedly, and the reason to close it rather than tolerate it.

## What to do

Add a cheap body sanity check after the status check, for each downloaded artifact:

- Verify the file's **magic bytes** match what the caller expects (zip `PK\x03\x04`, xz `\xFD7zXZ`, gzip
  `\x1f\x8b`). `file -b` is available on all three runners, or check the bytes directly — decide which and
  record why.
- Consider a **minimum plausible size** as a second cheap signal — a 197-byte "archive" is not one. Size
  alone is weaker than magic bytes; use it as a supplement, not a substitute.
- A **published checksum** would be strictly better than either, if the upstream publishes one per asset.
  Check whether BtbN and bblanchon do; if they do, prefer that and say so.

Whatever is chosen, the failure message must keep CPE-1762's standard: name the URL, say what was expected
versus what arrived, and never let the bad bytes reach `tar`/`unzip`.

## Acceptance criteria

- [ ] A 200 response carrying a non-archive body fails the guard, with a message naming the URL and saying
      the body is not the expected archive type. Demonstrate with real output against a live 200-serving
      HTML URL, as the UAT did.
- [ ] Every currently-guarded download (pdfium on all three OSes, ffmpeg on Windows and Linux) gets the
      check, with the expected type per call site — not a single hardcoded type.
- [ ] A genuine, live download still succeeds and is not slowed meaningfully.
- [ ] A truncated-but-200 body is considered: state whether the chosen check catches it, and if not, say so
      explicitly rather than leaving it implied.
- [ ] The macOS ffmpeg leg (build-from-source via `git clone`) is confirmed out of scope, in writing.

## Notes

Related: **CPE-1762** (the status-code guard this extends; PR #922), **CPE-1763** (the scheduled freshness
check), CPE-1258 (introduced the native-deps staging step).

Filed by the Foreman during the batched sprint of 2026-08-17, from CPE-1762's UAT break-attempt findings.
