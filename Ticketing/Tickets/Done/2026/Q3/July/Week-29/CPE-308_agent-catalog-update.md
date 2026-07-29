---
id: CPE-308
title: Agent catalog update / subscription
type: Feature
status: Done
priority: Medium
component: Backend
tags: [big-design]
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-14
---

## Summary

"Keep up with the coding-agent market without ricochet" needs the catalog itself to
refresh without an app release. Let the bundled agent/provider/plugin manifests
update from a signed remote source (or a user-pointed one), so new agents and
changed install recipes arrive as data.

## Acceptance Criteria

- [x] Fetch/refresh catalog manifests from a configured source; signature-verified
      ([[CPE-295]]) before trust.
- [x] New/updated manifests slot into the registry ([[CPE-278]]) with no code change;
      schema-migrated if needed ([[CPE-300]]).
- [x] User controls: manual refresh, auto-update toggle, pin/rollback a manifest
      version.
- [x] Offline-safe: last-known-good catalog keeps working ([[CPE-310]]).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]], [[CPE-295]]. **Phase:** C6. **Epic:** [[CPE-261]].

## Resolution

Closed as the umbrella feature: every acceptance criterion is delivered and test-backed by the
child slices **CPE-371** (per-manifest signed source), **CPE-372** (signed host index + anti-rollback),
**CPE-373** (`apply_bundle`, last-known-good), **CPE-374** (refresh UI), **CPE-375** (runtime reload),
**CPE-376** (host-mediated GitHub-Releases fetch), **CPE-377** (CI sign+publish + docs), **CPE-378**
(auto-update toggle + pin), and **CPE-379** (reset-to-shipped rollback). No further code was required.

**Operational note (not an unmet AC):** the pipeline is **dormant-safe** until an operator generates
the catalog **signing key** (per README) and CI publishes a signed bundle — analogous to the
code-signing-cert gate. The *mechanism* is complete and verified; enabling it is a deployment step.

**Follow-on (optional enhancement, not required here):** specific-**version** rollback (enumerate prior
published versions + audited downgrade override) is tracked as backlog **[[CPE-383]]**. This ticket's
AC only requires pin + reset-to-shipped rollback, both shipped.

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
2026-07-14 — **Part 2 pipeline complete:** reload (CPE-375), host-mediated GitHub-Releases fetch
(CPE-376), CI sign+publish + docs (CPE-377), and the "Update agents" refresh UI (CPE-374) all
landed. The feature is wired end-to-end and **dormant/safe** until an operator generates the catalog
signing key (README) — then the CI publishes a signed bundle and the app fetches/verifies/applies it.
Only advanced controls remain: **CPE-378** (auto-update toggle + pin/rollback, `big-design`).
2026-07-14 — **CPE-378 landed** (auto-update toggle + per-agent pin; apply skips pinned). Only
explicit rollback remains — **CPE-379** (`big-design`: release enumeration + audited downgrade).

2026-07-14 — Picked up to reconcile the parent feature against its now-landed slices. Estimate: 3-4h
(unchanged; delivered incrementally). Verified every acceptance criterion is satisfied by shipped,
tested code:
  1. **Fetch/refresh + signature-verified before trust** — `AgentRegistry::load_signed_source`
     (sidecar per-manifest `.sig`, CPE-371), host-authoritative signed **index** `verify_index` +
     content-binding (CPE-372), `apply_bundle` gating (CPE-373), runtime reload (CPE-375), and the
     host-mediated GitHub-Releases fetch (CPE-376).
  2. **Slot into the registry, schema-migrated** — signed manifests override by id additively; unknown
     future-schema manifests are skipped (CPE-300 discipline in `agents.rs::validate`).
  3. **User controls** — manual refresh UI (CPE-374), auto-update toggle + per-agent **pin** (CPE-378),
     and **rollback**: reset-to-shipped, "the simplest, safest rollback" (`console.rs:670`, CPE-379,
     Done). Specific-*version* rollback is an optional enhancement beyond this AC, tracked as the
     separate backlog ticket **CPE-383**.
  4. **Offline-safe last-known-good** — `apply_bundle` writes nothing on a bad index/signature; a bad
     source never clobbers the good catalog (`a_bad_index_signature_touches_nothing_last_known_good`).

2026-07-14 — Verified green: `cargo test -p ai-console` (128 + 7 catalog contract) and
`cargo test -p host --lib` (**91 passed, 0 failed**), including the catalog trust/apply/anti-rollback/
pin/last-known-good/sign-round-trip tests. No new code needed for this close — the pipeline was
completed across CPE-371–379.
