---
id: CPE-515
title: "Swarm — role/team manifest model (coordinator/builder/scout/reviewer)"
type: Feature
status: Open
priority: Medium
component: Sidecar
tags: [ready]
estimate: 2-3h
created: 2026-07-16
epic: CPE-502
sprint: SPR-01
---

## Summary
Declarative **team templates** for Swarm ([[CPE-502]], wave 1): a team is a **manifest** listing roles
(**coordinator / builder / scout / reviewer**), each bound to an agent + model, reusing the agent-registry
pattern ([[CPE-278]]). Predictable, shareable, versionable.

## Acceptance Criteria
- [ ] A team-manifest schema (roles + per-role agent/model + counts) parses + validates; bad manifests
      fail with clear errors (total, no panic).
- [ ] At least one shipped default template (e.g. coordinator + 2 builders + reviewer).
- [ ] Roles are a closed vocabulary (coordinator/builder/scout/reviewer) with defined responsibilities.
- [ ] Reuses / mirrors the CPE-278 agent-registry loading pattern where sensible.
- [ ] Unit tests for parse + validation + the default template.

## Notes
Wave 1 of [[CPE-502]]; declarative-manifest role model per the activation decision. Consumed by the
coordinator (CPE-517).
