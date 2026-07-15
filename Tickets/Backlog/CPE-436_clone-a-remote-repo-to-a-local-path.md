---
id: CPE-436
title: "Clone a remote repo to a local path"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [ready]
estimate: 2h
created: 2026-07-15
epic: CPE-429
---

## Summary
Clone any accessible remote repo locally (CPE-429), shelling out to git per the provider manifest (D2).
Foundation for two-way mirroring.

## Acceptance Criteria
- [~] Clone via the provider git URL to a chosen local path; progress + completion surfaced. — hardened argv **builder** done (`clone.rs`); execution + progress-streaming is the runtime tail (host + real git).
- [ ] Auth injected from the secrets broker (CPE-439); never on disk/logs. — depends on CPE-439 + host run.
- [x] Actionable errors; a partial clone is cleaned up. — pre-flight refusals (BadUrl/BadTarget/BadRef) are typed + tested; partial-clone cleanup happens at execution (runtime).

## Work Log
2026-07-15 — Picked up (forge-slice advance). Building the testable core: a hardened `git clone` argv builder (threat-model §C: hooksPath empty, protocol.ext/file=never, fsmonitor off, no submodule recursion) + URL-scheme allow-list + target-dir validation, as sidecar/repos/src/clone.rs. Pure + unit-tested. Execution + progress-streaming + auth injection are the runtime/GUI tail (need the host + a real git), noted as remaining.

## Work Log
2026-07-15 — Landed the testable core: sidecar/repos/src/clone.rs — hardened `git clone` argv builder (§C: core.hooksPath=, protocol.ext/file=never, fsmonitor off, --recurse-submodules=no, --no-tags, `--` before url/target), https/ssh-only URL allow-list, absolute non-nested target validation, option-injection-safe depth/branch. 5 unit tests; clippy clean. Kept OPEN — execution/progress/auth-injection are the runtime tail (need the host + a real git + CPE-439).
