---
id: CPE-823
title: Document the decoupled-server architecture under docs/design
type: docs
component: Docs
priority: low
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-810
estimate: 1h
---

## Summary
Fulfil the CPE-810 epic Definition of Done item: *"The architecture (contract, Server split, transport
seam, security planes) is documented under `docs/design/`."* The pieces now exist (`cpe-contract` CPE-811,
`cpe-security` CPE-816-818, the `ServerCtx` seam CPE-814, and the extracted `cpe-server` crate CPE-815/821/
822) but there's no single design doc explaining how they fit together or the recipe for extracting more.
Write `docs/design/SERVER-ARCHITECTURE.md` in the house style of the other `docs/design/*` standards, and
reference it from CLAUDE.md's docs section.

## Acceptance Criteria
- [x] `docs/design/SERVER-ARCHITECTURE.md` covers: the GUI ⟂ transport ⟂ Server split, the `cpe-server`
      crate + `ServerCtx` seam, the `cpe-contract` envelope, the `cpe-security` planes, the streaming
      split, and the "extract a domain" recipe (with the app-slimming payoff).
- [x] Referenced from CLAUDE.md ("How the pieces connect"), pointing new domain logic at `cpe-server`.
- [x] Marks what's shipped vs planned (frontend transport seam CPE-819 + remote loop CPE-820 + typed
      bindings CPE-812/813 are planned; contract/security/ServerCtx/extraction are shipped).

## Work Log
2026-07-21 — Wrote `docs/design/SERVER-ARCHITECTURE.md` in the house style of the other design standards:
the three-topology split, the three standalone crates (`cpe-contract`/`cpe-security`/`cpe-server`), the
`ServerCtx` seam (+ `TauriCtx`/`HeadlessCtx`), the three security planes with default-deny + local-null,
the "command = thin dispatcher" rule, how streaming survives the split, the extraction recipe, and the
17-crate app-slimming payoff. Added a bullet to CLAUDE.md's "How the pieces connect" pointing new domain
logic (and new format crates) at `cpe-server` and linking the doc. Fixed the module count to 27.

## Resolution
Fulfils the CPE-810 epic DoD item *"the architecture is documented under `docs/design/`."* Files:
`docs/design/SERVER-ARCHITECTURE.md` (new) + a CLAUDE.md reference. Docs-only — no code touched.
