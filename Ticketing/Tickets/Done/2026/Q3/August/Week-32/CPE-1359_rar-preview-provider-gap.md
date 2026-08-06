---
id: CPE-1359
title: "RAR preview gap: .rar isn't in provider.ts ARCHIVE_EXT, so selecting a .rar shows the hex view instead of its entry list"
type: Bug
status: Done
priority: Medium
component: frontend
tags: [ready]
epic: CPE-111
created: 2026-08-06
closed: 2026-08-06
---

## Problem

CPE-1348 wired RAR listing into the backend (`read_archive_entries` → `crate::rar::rar_entries`) and added
`"rar"` to `ARCHIVE_EXTS` in `src/lib/archiveExts.ts` (the browse-action / extract gating set). But the
**preview provider** uses a SEPARATE list — `ARCHIVE_EXT` in `src/lib/preview/provider.ts` (~line 56):
`{zip, jar, apk, war, ear, ipa, xpi, tar, tgz, gz, 7z, iso}` — which does **not** include `rar`. So a `.rar`
selected in the preview pane does NOT match the `archive` provider and falls through to the generic
`info`/hex provider — it shows a hex dump instead of listing its entries "like a zip." (Found by the CPE-1358
sample-coverage audit, which had to use a `.zip` for the archive category because `.rar` doesn't resolve to
the archive kind.)

## Fix

Add `"rar"` to `ARCHIVE_EXT` in `src/lib/preview/provider.ts` so `.rar` resolves to the `archive` preview
kind and lists its entries via `read_archive_entries` (which already handles RAR). Verify the archive preview
component renders the RAR entry list.

- Check whether any OTHER extension is similarly out of sync between `archiveExts.ts` (ARCHIVE_EXTS) and
  `provider.ts` (ARCHIVE_EXT) — ideally the two should share one source of truth so this can't drift again
  (consider importing ARCHIVE_EXTS, minus the extract-only distinction, into the provider). At minimum,
  reconcile `rar`.
- Update `provider.ts`'s doc comment (currently "zip family, tar, gzip") to include RAR.
- The `samples/archives/sample.rar` fixture (CPE-1341-lane) then genuinely covers the archive category too.

## Acceptance criteria

- Selecting a `.rar` in the preview pane lists its entries (archive kind), not a hex dump.
- provider vitest updated to assert `.rar` → `archive` kind; `src/lib/sampleCoverage.test.ts` still passes.
- `npm run check` + JS suite green. No backend change (rar_entries already dispatched from read_archive_entries).

## Notes

Found 2026-08-06 via the CPE-1358 QA harness — exactly the kind of cross-list drift the sample-coverage
ratchet + gui-smoke walk are meant to surface. Epic CPE-111. Small frontend fix.

## Work Log
- 2026-08-06: PR #653 merged (2a200f52). Added "rar" to ARCHIVE_EXT in provider.ts so .rar lists entries via the archive provider (read_archive_entries already dispatches to rar_entries) instead of the hex dump. Coverage ratchet flagged sample.rar was the only hex cover -> added samples/other/blob.pak + gen_samples.py make_hex_blob generator (byte-reproducible). provider.test.ts asserts rar->archive. Reviewer APPROVE (rar->archive + blob.pak->hex verified, generator byte-match, ratchet mutation-killed). Foreman-applied. Fixed stale sampleCoverage.test.ts comment (rar->archive now).
