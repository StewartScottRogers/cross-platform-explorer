---
id: CPE-1491
title: "File split / join — chunk a large file into parts and rejoin (classic commander utility)"
type: Feature
status: Backlog
priority: Low
component: Multiple
tags: [ready]
created: 2026-08-08
---
## What
A small, classic orthodox-commander utility (Total Commander / Multi Commander staple, absent from CPE):
**split** a large file into N fixed-size parts (`.001`, `.002`, … + a small checksum/index), and **join** them
back into the original. Still genuinely useful for FAT32/USB size limits, chunked uploads, and email-attachment
splits. Surfaced by the competitive-landscape GUI survey.

## Honest framing (why it's Low)
Least differentiating item from the survey — a CLI/script can do the same job and the GUI value-add is modest.
Filed as a cheap, well-scoped Low ticket, not an epic. Build if the queue wants an easy backend win; don't
prioritize it over the differentiators (CPE-1487/1488/1489 or activating CPE-661/616).

## How
- Backend (`cpe-server`): a **stream-chunking** module — split reads the source with a **bounded/streamed**
  reader (never load the whole file; follow STREAMING.md + the resource-exhaustion conventions) writing
  fixed-size parts + a tiny manifest (part count, sizes, sha256 of the whole for verify); join concatenates
  parts in order and verifies the checksum. No new Cargo deps (reuse the existing sha256 from CPE-412/737).
- Frontend: one dialog (choose part size / pick parts to join) + context-menu entries (MENUS standard); the
  actual work runs through the transfer/progress surface where it fits.

## Verify (headless half is clean)
`cargo test`: round-trip (split then join == original bytes, checksum matches); odd final part size; a part
missing/corrupt → join errs gracefully; bounded on a large synthetic input (no full-file buffer). `cargo
clippy --all-targets -D warnings`.

## Effort
Small. Backend split/join + fixtures is headless-buildable and a good batch; the dialog is the GUI half.
