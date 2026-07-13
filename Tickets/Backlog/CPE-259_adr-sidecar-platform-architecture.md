---
id: CPE-259
title: "ADR: Sidecar platform architecture"
type: Task
status: Open
priority: High
component: Docs
estimate: 2-3h
created: 2026-07-13
---

## Summary

Write the architecture decision record that governs the whole sidecar effort.
Mega-Features (starting with the AI Console) are built as **standalone sidecar
processes** hosted by the explorer through **one versioned contract**. This ADR
is the constitution the two epics ([[CPE-260]] platform, [[CPE-261]] AI Console)
must obey; nothing is built until it is agreed.

## Acceptance Criteria

- [ ] Records the five principles: standalone-capable, built-for-hugeness, zero
      ricochet, plugin-but-more-flexible, no-entanglement/ultra-clear-boundaries.
- [ ] States the **invariants**: one-way dependency (sidecars target the contract,
      never the explorer; explorer never imports sidecar internals); each sidecar
      is its own process / crash domain / storage & secrets scope; sidecars cannot
      see each other except via host-brokered channels.
- [ ] Defines the **delete-test** as the enforcement rule: remove all sidecars →
      explorer still builds, ships, runs fast/small/predictable; remove one →
      nothing else notices.
- [ ] Records decisions: isolation model = **sidecar (out-of-process)**; UI mount
      = **each sidecar serves its own UI, host embeds it**; contract is small,
      additive, semver'd, negotiated at handshake.
- [ ] Explains the relationship to Agent Watch (observe-only) vs the AI Console
      (drives an agent) — distinct surfaces.
- [ ] Lives as `docs/adr/0001-sidecar-platform.md` (or agreed path), linked from
      PURPOSE.md/CLAUDE.md.

## Notes — Dependencies / Schedule

**Depends on:** none. **Phase:** Foundation (must land first). Blocks all of
CPE-260 and CPE-261.

## Work Log
2026-07-13 — Filed during Nightshift epic planning. First ticket of the sidecar
platform program.
