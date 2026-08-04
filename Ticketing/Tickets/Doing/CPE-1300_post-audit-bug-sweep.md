---
id: CPE-1300
title: "Post-audit bug sweep of cpe-server modules added since 2026-07-21"
type: chore
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
The last pure-logic audit (2026-07-21) predates the safety scans, `read_wav`/`read_xmp`/`read_iptc`, the
new metadata columns, and the streaming walkers. Re-audit ONLY the newer modules for the two bug patterns
that audit flagged: (1) str-slice-at-byte-offset panic (slicing a `&str`/`&[u8]` at an offset not proven
in-bounds / not on a char boundary), and (2) a dead truncation notice after a capped read (a `truncated`
flag that can never be set, or is set wrong). Deliver a concrete fix + regression test where a real issue is
found, or a one-line "audited clean" note per module.

## Build
- Audit these modules for the two patterns (read-mostly; any fix is small + localized + gets a regression
  test): `type_mismatch_scan.rs`, `empty_dirs_scan.rs`, `orphan_sidecars_scan.rs`, `dangling_links_scan.rs`,
  `archive_safety_scan.rs`, `media_meta_read.rs` (the `read_wav`/`read_xmp`/`read_iptc` paths),
  `text_encoding.rs`. Also glance at the new streaming walkers' cap/`truncated` handling.
- For any real bug: fix it minimally + add a cargo test that fails-on-old/passes-on-new. For clean modules:
  note "audited clean (patterns 1+2)" in the work log — do NOT invent churn.
- No new dep. No behavior change beyond a genuine bug fix.

## Acceptance criteria
- Each listed module is audited; every finding is either fixed-with-regression-test or explicitly noted
  clean. `cargo test -p cpe-server` green; `cargo clippy` clean both feature modes.
- Honest: value only if it finds something — an all-clean result is a valid, reported outcome.

## Notes
Epic CPE-1002 (safety/robustness). Read-mostly; sequenced after the streaming refactors settled those files.
Bug patterns from [[cpe-server-logic-audited]].

## Work Log
