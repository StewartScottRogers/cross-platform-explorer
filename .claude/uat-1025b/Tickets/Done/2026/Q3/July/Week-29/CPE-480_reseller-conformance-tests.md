---
id: CPE-480
title: "Task"
type: ready
status: Done
priority: Medium
component: Sidecar (AI Console)
tags: [1-2h]
estimate: Reseller conformance tests + CI
created: 2026-07-15
closed: 2026-07-16
epic: CPE-467
---

## Summary
A conformance kit that validates every reseller manifest: descriptor parses, both protocol templates
fill correctly, egress host is allow-listed, and the model-list shape normalizes. Runs in CI so a
malformed reseller cannot ship.

## Acceptance Criteria
- [x] A data-driven test iterates every `resellers/*.json` and asserts descriptor + egress + recipe-fill.
- [x] A reseller added purely as data passes with no code change (proves the extensibility claim).
- [x] Wired into CI (both feature modes); clippy clean.

## Resolution
Added a data-driven conformance kit `sidecar/ai-console/tests/reseller_conformance.rs` (2 tests):
1. **`every_bundled_reseller_manifest_is_valid_and_self_consistent`** — loads the real `resellers/`
   dir and asserts, for all 19 manifests: none skipped as malformed; a healthy count (>=15); each has
   >=1 `api_hosts`; the `models_endpoint` host is IN `api_hosts` (so the host allow-list, keyed on the
   same hosts, covers the fetch); a launch-capable reseller derives a descriptor with an https base
   whose host is in `api_hosts`; and a model-list-only reseller does NOT half-declare launch fields
   (protocol XOR launch_base_url).
2. **`a_reseller_added_as_pure_data_is_launch_capable_with_no_code_change`** — the CPE-467
   extensibility proof: a fresh valid manifest becomes a launch descriptor with zero code.

All 19 bundled resellers pass. `cargo clippy --all-targets -D warnings` clean. (The host-side
`models_egress` allow-list has its own every-reseller test in src-tauri.) Nightshift loop 8.

Research note: while looking for a new explorer feature this loop, confirmed the obvious ones already
exist — type-ahead find (`firstMatchIndex`), natural-order sort (Intl.Collator numeric, folders-first),
invert-selection, new-file, and the status-bar selection summary — so the explorer is mature; the
highest-value remaining work is the reseller epic wrap-up + i18n, hence this conformance kit.
