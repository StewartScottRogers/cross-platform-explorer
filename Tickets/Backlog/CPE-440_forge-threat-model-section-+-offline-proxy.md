---
id: CPE-440
title: "Forge threat-model section + offline/proxy"
type: Task
status: Open
priority: High
component: Multiple
tags: [needs-prereq]
estimate: 1-2h
created: 2026-07-15
epic: CPE-429
---

## Summary
Security review for the forge sidecar (CPE-429), extending CPE-304: token safety, per-provider
allow-listed egress, untrusted-clone-to-disk consent, and enterprise proxy/offline (reuse CPE-310).

## Acceptance Criteria
- [x] Threat-model section: egress (one row per provider host), token in transit/at rest/logs,
      clone/pull brings untrusted content (consent/scan). — `docs/security/forge-threat-model.md` §A–§D/§G.
- [~] Offline + corporate-proxy honoured (reuse CPE-369/310). — **documented as a required invariant**
      (§E, reusing `keyverify` `is_offline`/`resolve_proxy`/`host_matches_no_proxy`); the actual
      honouring lands with the egress/clone code (CPE-433/436) — not yet wired.
- [ ] Recorded in ADR 0001 once the vertical slice is verifiable. — gated on CPE-433/434/436/439/435.

## Resolution (partial — threat-model authored; kept open, `needs-prereq`)
Wrote `docs/security/forge-threat-model.md` (CPE-440): a **design-stage** STRIDE review of the forge
tenant's *new* surfaces on top of the CPE-304 platform review — multi-host allow-listed egress incl.
user-supplied self-hosted hosts (§A, SSRF/rebinding/metadata guards), untrusted-response parsing (§B),
untrusted **clone-to-disk** (§C, hooks/filters/alt-transport off + consented target + never-execute),
two-way **push/exfiltration + force-push** (§D, safe-by-default planner), and offline/proxy reuse (§E).
§G pins the invariants each build slice (433/434/436/439/435) must satisfy; §H maps sign-off blockers;
§I records **NOT SIGNED OFF — design-stage**. AC1 done. AC2 documented (wiring pending 433/436). AC3
(ADR record) intentionally deferred until the vertical slice is runtime-verifiable — this is the
`needs-prereq` gate, an internal-work prereq (not an external block), so the ticket stays in Backlog.

## Work Log
2026-07-15 - Picked up (work-all). Estimate 1-2h. Plan: write a dedicated forge threat model (docs/security/forge-threat-model.md) in the CPE-304 STRIDE format covering the NEW surfaces the repos sidecar adds — multi-host allow-listed egress, forge tokens/SSH keys, untrusted clone-to-disk, two-way push/exfiltration, offline+proxy. AC3 (ADR 0001 record) stays open until the vertical slice (433/434/436/439/435) is runtime-verifiable.

## Work Log
2026-07-15 - Authored docs/security/forge-threat-model.md (design-stage STRIDE, extends CPE-304). AC1 done; AC2 documented (wiring pending CPE-433/436); AC3 (ADR record) deferred to the verifiable vertical slice. Kept open in Backlog (needs-prereq, internal-work gate).
