---
id: CPE-315
title: Expand the seed agent catalog (more reference agents)
type: Task
status: Done
priority: Low
component: Backend
estimate: 1h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

CPE-291 seeded 8 agents and noted the rest as pure-data follow-ups. Add the further
reference agents whose install/run commands are unambiguous, as bundled manifests.

## Acceptance Criteria

- [ ] Add manifests for agents with clean, public install commands (codebuff, pi, tau,
      vtcode). Skip ones needing local spec-vars/WSL (opensquilla, trae, amazonq, …)
      until their commands are pinned.
- [ ] The catalog integration test covers the additions (loads clean, all have
      run/install/uninstall, every provider routes).
- [ ] `cargo test` + clippy pass.

## Work Log
2026-07-13 — Filed and picked up during dayshift (was too quick to call the catalog done).
2026-07-13 — Added codebuff, pi, tau, vtcode (accurate public install commands from the reference: npm/uv/cargo). Skipped opensquilla/trae (local spec-vars), amazonq (WSL), antigravity/hermes/junie/oh-my-pi (no pinned public command). Catalog now 12 agents; integration test extended + green. Done.
