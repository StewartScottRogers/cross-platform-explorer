---
id: CPE-584
title: "AI Console: populate the model picker live from LM Studio when lmstudio-local is selected"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
When the AI Console's **Provider** is set to `lmstudio-local`, the Model picker should populate
**dynamically from LM Studio's loaded models** (its OpenAI-compatible `GET /v1/models`) instead of
falling back to the empty/openrouter list. Selecting LM Studio already auto-detects the endpoint +
loaded model at launch (CPE-286/330); this extends that so the picker itself lists every loaded model.

## Approach
LM Studio runs on **localhost** (`127.0.0.1:1234/1235`), so the sidecar fetches it directly — no host
egress broker needed (matching the existing launch-time TCP probe in `lmstudio.rs`). Reuse the existing
OpenAI-compatible normalizer.

- `lmstudio.rs` — factor the probe's TCP GET into a shared `http_get`, add `fetch_models_body(base_url)`
  returning the raw `/v1/models` JSON (HTTP headers stripped).
- `console.rs::handle_models` — when `reseller == lmstudio-local`, detect the reachable endpoint
  (`detect_default`), fetch its `/v1/models`, and return `normalize_models("lmstudio-local", body)`.
  Gracefully returns an empty list when LM Studio isn't running.
- Frontend already queries `/api/models?reseller=<provider>` on provider change — no change needed.

## Acceptance Criteria
- [x] `lmstudio.rs` gains a reusable localhost `GET` + `fetch_models_body`, with the HTTP header strip
      unit-tested.
- [x] `handle_models` serves LM Studio's loaded models for `lmstudio-local` (live, localhost), empty
      when unreachable.
- [x] Selecting `lmstudio-local` in the AI Console lists the loaded models in the picker (frontend
      already re-queries `/api/models?reseller=<provider>` on provider change — no change needed).
- [x] Full sidecar suite (**284 passed**) + `clippy --all-targets -D warnings` green.

## Resolution
- `lmstudio.rs` — factored the launch-time probe's TCP call into a shared `http_get(base_url, path,
  timeout)`; added `fetch_models_body` (returns the `/v1/models` JSON with HTTP headers stripped via
  `strip_http_headers`). Tests: header strip (CRLF/LF/no-header) + an LM Studio body normalizing to the
  picker list.
- `console.rs::handle_models` — early branch for `reseller == lmstudio-local` → new
  `handle_lmstudio_models`: `detect_default()` (loopback then LAN, ports 1234/1235) → `fetch_models_body`
  → `normalize_models("lmstudio-local", body)`. Returns a well-formed **empty** list when LM Studio isn't
  running, so the picker shows "no models available" rather than an error.
- Reused the existing OpenAI-compatible normalizer (`parse_openrouter_models` reads `data[].id`, tolerating
  LM Studio's id-only rows), so no new parser.
- No frontend change: `populateModels()` already sends the selected provider as `reseller` and refreshes on
  provider change.

**Verified:** unit tests green against LM Studio's exact `{data:[{id}]}` shape (confirmed live via curl);
the fetch reuses the same TCP/HTTP path already proven in production for launch-time detection. Ships in
the next sidecar build (0.34.0 predates it).

## Notes
Zero-key, zero-cost — LM Studio is local. Ties into the swarm work (a free provider for CPE-582).
