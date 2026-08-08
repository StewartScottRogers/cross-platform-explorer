---
id: CPE-1439
title: "Some archive extensions (xz/bz2/zst/dmg/cab/lz/lzma) fall through to hex preview instead of archive listing"
type: Bug
status: Backlog
priority: Low
component: Full-stack
tags: [ready]
epic: CPE-705
created: 2026-08-07
---
## Observation (from the CPE-1433 integration sweep; PRE-EXISTING, not this session)
`xz`, `bz2`, `zst`, `dmg`, `cab`, `lz`, `lzma` are categorized `"archive"` in `CATEGORY_BY_EXT`
(`src/lib/filetypes.ts`) but are NOT in `provider.ts`'s `ARCHIVE_EXT` set, so they fall through to the `hex`
preview instead of the archive-listing provider.

## Investigate before building (may need backend work, not just a provider tweak)
Do NOT just add them to `ARCHIVE_EXT` blindly — check whether the backend archive lister (`crates/server/src/archive.rs`,
which currently handles zip/tar/gz/7z/iso/rar) can actually list/handle each:
- `xz`/`bz2`/`zst`/`lz`/`lzma` are single-file COMPRESSION formats (often `.tar.xz` etc.) — a bare `.xz` has no
  entry list; the right preview may be "compressed file (decompressed size N)" or the inner tar if it's `.tar.xz`.
- `dmg` (Apple disk image) / `cab` (MS cabinet) are containers needing their own readers — likely out of scope /
  gold-plating unless a reader already exists.
Scope to only the ones the backend can genuinely list, with a graceful fallback for the rest. If none are
cheaply supportable, close as won't-fix rather than routing them to a lister that errors.

## Notes
Low priority, pre-existing (not caused by the structured-preview or media work). Filed so the observation isn't
lost. Verify backend capability first.
