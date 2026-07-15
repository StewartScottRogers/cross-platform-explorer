---
id: CPE-383
title: "Catalog rollback to a specific prior version (enumeration + downgrade override)"
type: Feature
status: Done
priority: Low
component: Multiple
tags: [big-design]
estimate: 4h+
created: 2026-07-14
closed: 2026-07-14
---

## Summary

Beyond reset-to-shipped (CPE-379): roll an agent back to a specific *previously-published* catalog
version.

## Acceptance Criteria

- [x] Enumerate prior published versions (GitHub Releases API — new allow-listed egress).
- [x] Fetch a specific older signed bundle (`releases/download/<tag>/…`, not `latest`).
- [x] Apply with a deliberate, audited **downgrade override** for the chosen agent(s) only
      (an `allow_downgrade` path in `apply_bundle`, not a blanket flag).
- [x] UI: version picker + per-agent provenance/version display.

## Notes — why `big-design`
Deliberately defeats the CPE-372 anti-rollback invariant, so it needs a careful override + a trusted
source of the older bundle + release enumeration. Depends on [[CPE-379]]/[[CPE-376]]. Part of [[CPE-308]].

## Resolution

Delivered the full "roll an agent back to a specific published version" flow across all layers:
- **host apply** (`sidecar/host/src/catalog.rs`): `gate_manifest_opt` / `apply_bundle_with` with a
  narrow, per-agent `allow_downgrade` that relaxes **only** anti-rollback (signatures + content
  binding still enforced; pin wins; non-opted agents unaffected).
- **host egress** (`src-tauri/src/lib.rs`): GitHub Releases **API** enumeration + a **version-specific**
  `releases/download/<tag>/` fetch with a strict tag-safety check; both host-built (no SSRF), proxy/
  offline-aware; a new `host.list_catalog_versions` method and a `{tag, agents}` extension to
  `host.fetch_catalog`.
- **sidecar** (`broker_client.rs`, `console.rs`): `HostDialogs` methods + `/api/catalog/versions`
  and `/api/catalog/rollback` routes.
- **UI** (`launcher.html`): a Manage-menu "Roll back a version…" picker overlay with per-version
  provenance (tag + date + pre-release), scoped to the selected agent.
- **security** (`docs/security/threat-model.md`): recorded `api.github.com` as an allow-listed egress.

**Tradeoff / minor deferral:** the picker shows the **available** versions' provenance (date /
pre-release) scoped to the chosen agent, but not a badge of each agent's **currently-installed**
catalog version — that would require surfacing the host-side version map (`versions.json`) into the
sidecar's `catalog()` payload, a small follow-on not needed to pick + roll back a version. The
override is dormant end-to-end until the operator configures the catalog signing key (same gate as
CPE-308), but every layer is built + tested.

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

2026-07-14 — Resumed and completed the remaining slices end-to-end (all in one pass):

**S2 — version enumeration (AC1).** `src-tauri/src/lib.rs`: `github_releases_api()` (host-built URL,
`api.github.com/repos/<repo>/releases`), `parse_release_versions` (pure — keeps only releases that
carry a `catalog-index.json` asset and a URL-safe tag; drops prose/traversal), `list_catalog_versions`
(offline ⇒ empty; proxy-aware via `catalog_http_get`, now sending a GitHub-required `User-Agent`
+ `Accept`), and the `host.list_catalog_versions` request handler. New **allow-listed egress**
(`api.github.com`) added to threat-model §9. **3 host unit tests**.

**S3 — version-specific fetch (AC2).** `catalog_url_for_tag(tag)` targets `releases/download/<tag>/`
(never `latest`); `is_safe_release_tag` refuses any tag that could escape the path. `do_fetch_catalog`
now takes `(pinned, tag, allow_downgrade)` and calls `apply_bundle_with(..., allow_downgrade)`;
`fetch_catalog_response` reads optional `{tag, agents}`. Tag-URL + safety unit-tested.

**Sidecar plumbing.** `broker_client.rs`: `CatalogVersion` + trait methods `list_catalog_versions`
/ `rollback_catalog` on `HostDialogs` (real `BrokerDialogs`, `NoopDialogs`, and the test stub).
`console.rs`: routes `GET /api/catalog/versions` + `POST /api/catalog/rollback {tag, agents}` (guards:
tag required, ≥1 agent), hot-reloading on apply. **1 console route test**.

**S4 — UI (AC4).** `launcher.html`: a "Roll back a version…" item in the Manage-agents menu opens a
`rollback-overlay` — a version picker (`GET /api/catalog/versions`) showing each version's tag +
published date + a pre-release marker (**provenance**), scoped to the selected agent (its name in the
header), applying via `POST /api/catalog/rollback`. **2 jsdom tests** (picker populates + posts
`{tag, agents:[agent]}` and closes; empty-list path shows a message, no overlay).

2026-07-14 — Verified the whole vertical green: `cargo test -p host --lib catalog` **14 passed**;
`cargo test -p ai-console` **142 passed**; `cargo clippy` clean on both crates and on the host with
`--features sidecar-platform --all-targets`; `npm run check` **0/0**; `npm test` **392 passed**
(incl. 17 launcher jsdom tests).
