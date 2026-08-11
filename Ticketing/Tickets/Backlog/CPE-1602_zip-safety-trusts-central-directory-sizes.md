---
id: CPE-1602
title: "Zip-bomb scan trusts the archive's own size metadata — a lying central directory reads as fully scanned and safe"
type: Bug
status: Backlog
priority: Medium
component: Backend
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Found by the independent reviewer on CPE-1591 (PR #809) while building adversarial archives. It is **not**
a regression from that ticket — it predates it (CPE-1281's original design) and was explicitly out of that
ticket's scope, which covered encrypted entries only. But it is a real evasion of the safety check, and the
reviewer demonstrated it rather than theorising it.

## The evasion (reproduced)
The reviewer built a real zip bomb — a 2,000,000-byte zero payload, honestly deflated, ratio ≈1023× — and
then hand-patched the `uncompressed_size` field in **both** the local file header and the central
directory down to 100 bytes. Result from the real scan:

```
entries_scanned = 1, unreadable = false, unreadable_entries = 0,
overall_ratio ≈ 0.05, dangerous = false
```

The user sees the plain green **"No zip-bomb risk detected"** banner — a fully-scanned, confident verdict —
for an archive that is a genuine bomb on extraction.

## Root cause
`crates/server/src/archive_safety_scan.rs` computes the expansion ratio from
`zip::ZipFile::size()` / `compressed_size()` — i.e. it **trusts the archive's own metadata** rather than
decompressing and counting real bytes. The module's doc comment already states this honestly, so the
behaviour is by design; the question this ticket raises is whether the design is good enough for a check
users act on before extracting something they downloaded.

## Fix options (decide when picked up)
1. **Cheap, catches naive forgeries**: cross-check the local-file-header size against the central-directory
   size and treat a mismatch as untrustworthy — reporting "couldn't be verified" rather than "safe", using
   the tri-state CPE-1591 just built. Does not catch a forger who patches both (as the reviewer did).
2. **Sound, costs I/O**: decompress each entry through a capped, streaming counter and stop at the ratio
   threshold — the only way to actually know. Must be bounded (a cap on bytes decompressed per entry and
   overall) or the scan itself becomes the bomb.
3. A hybrid: metadata first, then verify by decompression only for entries whose metadata looks suspicious
   or whose declared size is implausibly small for its compressed size.

Whatever is chosen, an archive the scan cannot vouch for must land in the **"could not be checked"** state
CPE-1591 added, never the safe banner.

## Acceptance criteria
- The reviewer's hand-patched archive above no longer produces a safe verdict.
- A genuinely safe archive still scans fast and still says so (do not make the common case slow).
- A crafted archive cannot make the *scanner itself* consume unbounded time or memory.
- Docs updated to state honestly what the check does and does not guarantee.

## Notes
Conflict surface: `crates/server/src/archive_safety_scan.rs`, `archive_safety.rs`,
`ArchiveSafetyDialog.svelte` if the states change, `src/docs/explorer-archives.md`. Model: sonnet
(escalate if the streaming-verification design gets gnarly).
