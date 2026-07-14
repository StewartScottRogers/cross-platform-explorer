---
id: CPE-308
title: Agent catalog update / subscription
type: Feature
status: Open
priority: Medium
component: Backend
tags: [big-design]
estimate: 3-4h
created: 2026-07-13
---

## Summary

"Keep up with the coding-agent market without ricochet" needs the catalog itself to
refresh without an app release. Let the bundled agent/provider/plugin manifests
update from a signed remote source (or a user-pointed one), so new agents and
changed install recipes arrive as data.

## Acceptance Criteria

- [ ] Fetch/refresh catalog manifests from a configured source; signature-verified
      ([[CPE-295]]) before trust.
- [ ] New/updated manifests slot into the registry ([[CPE-278]]) with no code change;
      schema-migrated if needed ([[CPE-300]]).
- [ ] User controls: manual refresh, auto-update toggle, pin/rollback a manifest
      version.
- [ ] Offline-safe: last-known-good catalog keeps working ([[CPE-310]]).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]], [[CPE-295]]. **Phase:** C6. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-14 — **Part 1 landed (CPE-371):** the trust gate + merge semantics — sidecar-local
signature verification (`catalog::verify_manifest`, format-compatible with CPE-295) and
`AgentRegistry::load_signed_source` (signed-only, override-by-id, additive/last-known-good), wired
behind `CPE_AICONSOLE_CATALOG`. **Part 2 designed:** `docs/design/CPE-308-agent-catalog-updates.md`
covers the remote subscription (host-authoritative fetch+verify, signed catalog index, key
distribution/TOFU, auto-update/pin/rollback, offline-safe + anti-rollback, and the threat-model
section). Stays `big-design` pending sign-off on the design's open questions (D1–D4); once signed
off it splits into the 4 `ready` slices listed there.
2026-07-14 — **Design signed off (D1–D4 as recommended); slice 1 landed (CPE-372):** host-side
signed catalog index — `verify_index` + content-binding + anti-rollback (`host::catalog`), plus the
threat-model row. Reconciled trust: the index (host) governs allowed ids/versions; per-manifest
`.sig` (sidecar, CPE-371) governs content authenticity. Remaining split to **CPE-373** (fetch/apply,
offline-safe, last-known-good) and **CPE-374** (user controls UI). Default first-party source URL
still open — CPE-373 runs against a local/configured source until one is hosted.
2026-07-14 — **Slice 2a landed (CPE-373):** `host::catalog::apply_bundle` — gate a staged bundle
against the signed index (content + anti-rollback), write accepted manifests, persist the version
map, last-known-good on failure; 9 host catalog tests. Remaining: **CPE-375** (remote fetch +
runtime reload — `needs-decision` on the source URL) and **CPE-374** (user controls UI). All the
pure catalog trust/apply logic is now built and tested; what's left is network egress + runtime
reload + UI.
