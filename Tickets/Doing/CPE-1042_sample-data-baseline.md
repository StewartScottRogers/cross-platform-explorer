---
id: CPE-1042
title: Pristine sample-data baseline + sandbox-copy helper
type: test
component: Multiple
priority: medium
status: Doing
tags: ready
created: 2026-07-25
estimate: 2-3h
---

## Summary
A committed, **pristine** `samples/` directory of small files with **known, documented metadata**, usable as
a stable baseline for manual/GUI checks (e.g. verifying the Metadata Studio) and as automated fixtures. The
samples are **never edited in place** — a helper copies them into a **git-ignored sandbox** for any
destructive test (like editing tags and saving).

- `samples/` organized by kind: `audio/` (mp3/flac/ogg), `images/` (jpg+EXIF, png), `documents/` (pdf),
  `video/` (mp4), `text/` (txt/md/json/csv/py). Each carries fixed, known metadata.
- **Synthetically generated** — a committed `scripts/gen_samples.py` reproduces every file deterministically
  (no copyrighted media). Minimal-but-valid: the app's codecs read the metadata correctly.
- `samples/README.md` records each file's exact baseline metadata + the pristine rule.
- Sandbox-copy helper: `scripts/new-sample-sandbox.ps1` + `.sh` copy `samples/` → a git-ignored
  `.sandbox/` dir for destructive testing; `.gitignore` excludes `.sandbox/`.
- A backend fixtures test reads each sample via `cpe_server::media_meta::read_all` and asserts the known
  baseline, so the fixtures also guard the read codecs against regression.

## Acceptance Criteria
- [ ] `samples/` committed with the files above; each media file's metadata matches the README baseline.
- [ ] `scripts/gen_samples.py` regenerates the tree byte-reproducibly; documented in the README.
- [ ] `scripts/new-sample-sandbox.*` copy into a git-ignored `.sandbox/`; `.gitignore` updated.
- [ ] A `cpe-server` test reads each sample and asserts its baseline metadata (green in CI, 3-OS matrix).
- [ ] No copyrighted content; all files are small.

## Work Log
2026-07-25 — Filed at the user's request: keep a pristine known-baseline sample set, copied into a testing
dir when modification is needed. Independent of the Metadata Studio branch (PR #358).
