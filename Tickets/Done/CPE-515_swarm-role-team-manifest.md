---
id: CPE-515
title: "Swarm — role/team manifest model (coordinator/builder/scout/reviewer)"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [ready]
estimate: 2-3h
created: 2026-07-16
epic: CPE-502
sprint: SPR-01
closed: 2026-07-16
---

## Summary
Declarative **team templates** for Swarm ([[CPE-502]], wave 1): a team is a **manifest** listing roles
(**coordinator / builder / scout / reviewer**), each bound to an agent + model, reusing the agent-registry
pattern ([[CPE-278]]). Predictable, shareable, versionable.

## Acceptance Criteria
- [x] A team-manifest schema (roles + per-role agent/model + counts) parses + validates; bad manifests
      fail with clear errors (total, no panic).
- [x] At least one shipped default template (e.g. coordinator + 2 builders + reviewer).
- [x] Roles are a closed vocabulary (coordinator/builder/scout/reviewer) with defined responsibilities.
- [x] Reuses / mirrors the CPE-278 agent-registry loading pattern where sensible.
- [x] Unit tests for parse + validation + the default template.

## Resolution
Added `sidecar/ai-console/src/swarm_team.rs` (new, pure, 7 tests) — declarative Swarm team templates.

- **`Role`** — a closed serde enum (`coordinator`/`builder`/`scout`/`reviewer`), each with a
  `responsibility()` one-liner for the UI/docs.
- **`RoleSpec`** — `{ role, agent, model?, count=1 }` (serde `#[serde(default)]` mirrors the
  `agents.rs` / CPE-278 manifest pattern; `count` defaults to 1, `model` optional).
- **`TeamManifest`** — `{ name, description?, roles }` with `parse(json)` (parse **and** validate),
  `validate()`, `agents_in_role(role)`, `team_size()`. Invariants: a named team with roles, **exactly
  one coordinator**, **≥1 builder**, every slot non-zero with a real agent — each violation a distinct
  `TeamError` (`Display` + `std::error::Error`), never a panic on malformed JSON or a bad role.
- **`default_team()`** — coordinator + 2 builders + reviewer, validated.

Verified: `cargo clippy --all-targets -D warnings` clean; 7 unit tests (valid parse, default team,
missing/dup coordinator, missing builder + zero-count + blank agent, blank name/no roles, malformed
JSON, serde round-trip). Consumed later by the coordinator (CPE-517). Second ticket of SPR-01.

## Work Log
2026-07-16 — Picked up (SPR-01 wave 1). Estimate: 2-3h.
2026-07-16 — Built the Role enum + RoleSpec + TeamManifest (serde parse/validate mirroring agents.rs) + default_team, with 7 tests. clippy clean. All ACs met.

## Notes
Wave 1 of [[CPE-502]]; declarative-manifest role model per the activation decision. Consumed by the
coordinator (CPE-517).
