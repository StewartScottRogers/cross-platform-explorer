---
id: CPE-383
title: "Catalog rollback to a specific prior version (enumeration + downgrade override)"
type: Feature
status: Open
priority: Low
component: Multiple
tags: [big-design]
estimate: 4h+
created: 2026-07-14
---

## Summary

Beyond reset-to-shipped (CPE-379): roll an agent back to a specific *previously-published* catalog
version.

## Acceptance Criteria

- [ ] Enumerate prior published versions (GitHub Releases API — new allow-listed egress).
- [ ] Fetch a specific older signed bundle (`releases/download/<tag>/…`, not `latest`).
- [x] Apply with a deliberate, audited **downgrade override** for the chosen agent(s) only
      (an `allow_downgrade` path in `apply_bundle`, not a blanket flag).
- [ ] UI: version picker + per-agent provenance/version display.

## Notes — why `big-design`
Deliberately defeats the CPE-372 anti-rollback invariant, so it needs a careful override + a trusted
source of the older bundle + release enumeration. Depends on [[CPE-379]]/[[CPE-376]]. Part of [[CPE-308]].

## Work Log
2026-07-14 — Split from CPE-379 (which delivered reset-to-shipped).

2026-07-14 — Picked up. Estimate: 4h+ (added). Studied the catalog trust chain (`host::catalog`):
`apply_bundle` gates each entry via `gate_manifest` → `is_upgrade_over` (the CPE-372 anti-rollback
invariant). Confirmed this is a 4-layer feature: the `allow_downgrade` override (host), GitHub-API
version **enumeration** + version-specific **fetch** (host egress; the catalog UI + fetch live in the
AI Console **sidecar** surface, not the explorer frontend), and a **version-picker UI**.

2026-07-14 — **Slice 1 landed: the audited downgrade override (AC3) — the `big-design` crux.** Added
`gate_manifest_opt` and `apply_bundle_with` to `sidecar/host/src/catalog.rs`; `gate_manifest` /
`apply_bundle` remain thin wrappers (zero churn to existing call sites/tests). The override is
deliberately narrow: it relaxes **only** the "must be strictly newer" rule, and **only** for the
per-agent ids passed in `allow_downgrade` — index-signature, per-manifest signature, and SHA-256
content binding are all still enforced, a **pin still wins** over a rollback request, and any id not
opted in still gets full anti-rollback. On accept it sets `installed` to the older version so later
normal fetches upgrade from there. **3 new tests** (opt-in-only downgrade + never relaxes content;
mixed bundle rolls back only the chosen agent while others stay anti-rollback; pin beats downgrade);
`cargo test -p host --lib catalog` **14 passed**, clippy clean.

2026-07-14 — Returned to Backlog as `big-design`: the safety-critical apply-side override is done +
tested, but the user-facing flow is not wired, so the ticket stays open. **Remaining slices** (each a
`ready` follow-up):
  - **S2 — version enumeration (AC1):** a host-mediated GitHub **Releases API** GET
    (`/repos/<owner>/<repo>/releases`) to list published catalog versions — a **new allow-listed
    egress host** (`api.github.com`), reusing the CPE-369 proxy/offline handling and the no-URL-from-
    sidecar rule; parse tag + published-at, no auth.
  - **S3 — version-specific fetch (AC2):** a `host.fetch_catalog` variant taking a `{tag, agents}` so
    the base URL is `releases/download/<tag>/…` (not `latest`), staging that bundle and calling
    `apply_bundle_with(..., allow_downgrade = agents)`.
  - **S4 — UI (AC4):** a version picker + per-agent provenance/version display in the AI Console
    launcher UI (jsdom-tested), invoking S2/S3.
