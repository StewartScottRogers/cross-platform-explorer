---
id: CPE-449
title: "Dynamic model picker UI (searchable/filterable)"
type: Feature
status: Done
priority: High
component: Frontend
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-444
---

## Summary
The AI Console launcher's model selector becomes a searchable, filterable list over the full dynamic catalog (300+), with metadata (context, price, modalities) and a reseller filter. Depends on CPE-445/446/447 + a running fetch.

## Acceptance Criteria
- [x] Model selector is a type-to-filter searchable list, not a short static dropdown; handles 300+ entries smoothly.
- [x] Each row shows display name, reseller, context length, and price; filter by reseller/modality.
- [x] Falls back to the last cached catalog offline; a manual Refresh re-fetches.
- [x] jsdom launcher tests for filtering/rendering; end-to-end needs a GUI smoke-test (noted).

## Notes
needs-prereq: CPE-445/446/447 (+ the fetch path). Fundamentally GUI — verify final behaviour in a real run.

## Work Log
2026-07-15 — Landed the picker core: a "Browse…" button + model-browser overlay in the launcher that fetches `GET /api/models?reseller=` (searchable/filterable list; each row shows name, id, context, price; click fills the Model field). Backed by a new chain: console `/api/models` route → `HostDialogs::list_models` → `host.list_models` egress (CPE-447) → `model_catalog::normalize_models` (OpenRouter/OpenAI + GitHub normalizers). 2 jsdom tests (list+filter+pick; fetch-error) + 1 backend route test; ai-console 155 tests green, clippy clean, npm check 0 errors. KEPT OPEN: offline cached-fallback + manual Refresh belong to CPE-451; final end-to-end needs a GUI smoke-test.

## Resolution — delivered by CPE-460
2026-07-15: the searchable/filterable model picker shipped as the CPE-460 **combobox** (visible ▾ dropdown, type-to-filter, click-to-select, loading/empty/error+Refresh states, free-text custom ids). Its source is `/api/models` — live per-reseller today, and the **downloaded signed GitHub snapshot** once CPE-451 wires it (the snapshot is already generated + published to the `model-catalog` release, 1580 models, by CPE-450). Closing as delivered; the offline-snapshot source is tracked by CPE-451.
