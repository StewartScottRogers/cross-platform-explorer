---
id: CPE-1273
title: "Configurable real embedder for content search (OpenAI-compatible /v1/embeddings)"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-02
epic: CPE-976
---

## Summary
Item 4 of the user's queue. Content search (CPE-1262/1263) uses the local dependency-free `FakeEmbedder`
(keyword-ish). Add a REAL, configurable embedder so the user can plug in a better model — WITHOUT the repo
shipping a key: an OpenAI-compatible `/v1/embeddings` client (works with a LOCAL server like LM Studio /
Ollama — no cloud key — OR OpenAI/others with a key). Decide-and-log: OpenAI-compatible is the de-facto
embeddings standard and fits the user's installed LM Studio.

## Build
- New real `Embedder` impl (e.g. `HttpEmbedder`) in cpe-server implementing the existing `Embedder` trait
  (`embed`/`embed_batch` — override `embed_batch` to POST all texts in one request). POST to `<baseUrl>/embeddings`
  (or `/v1/embeddings`) with `{model, input}`, parse `data[].embedding`. Use **`ureq`** (ALREADY a dependency —
  webdav + src-tauri use it; do NOT add a new HTTP dep). Feature-gate if needed to keep the lean base build clean.
  `dim()` = the length of the returned vectors (probe once / from the first response); handle errors (unreachable
  endpoint, bad key, bad response) as clear `Err` — never panic.
- Config (persisted): `{ enabled: bool, baseUrl: string, model: string }` + the API KEY stored via the existing
  keychain seam (`SecretAccess`/keyring, from the vault work — NEVER in plaintext settings.json). Default DISABLED
  → content search keeps using FakeEmbedder (current behavior unchanged).
- Wire `content_index_build` + `content_search` (crates/server + src-tauri commands) to construct the embedder from
  config: enabled+configured → HttpEmbedder, else FakeEmbedder. Switching embedders changes the vector dim → the
  persisted index (built with the old dim) becomes stale; CPE-1262 already returns Err/needs-build on dim mismatch —
  ensure that degrades to a clean "rebuild the index" prompt, not a crash. Consider keying the persisted index by
  embedder identity so a switch triggers a rebuild.
- Settings UI (SettingsDialog): an "AI content search" section — enable toggle, endpoint URL, model name, API key
  field (stored to keychain), and ideally a "Test connection" button (embeds a short string, reports ok/error).
  Honest copy: works with any OpenAI-compatible embeddings endpoint; a local server (LM Studio) needs no key.
- Docs (CPE-579): document enabling it + pointing at LM Studio or OpenAI.
- Commands + specta bindings (regen bindings.gen.ts if a struct crosses); capability if a new command needs it.

## Acceptance criteria
- cargo build/test/clippy (all feature modes) clean; npm run check clean; bindings drift guard green; CPE-1271
  bundle guard green; no NEW dependency (reuse ureq + keyring).
- Unit tests: HttpEmbedder request build + response parse (mocked HTTP, no network), config→embedder selection,
  fallback to FakeEmbedder when disabled, dim from response, error handling (never panic). API key never logged/persisted in plaintext.
- Content search uses the real model when enabled+configured; falls back cleanly otherwise; switching prompts a rebuild.

## Notes
End-to-end (real embeddings → better search quality) is the USER's step: they point it at their LM Studio (local)
or OpenAI (key). Surface that clearly. The pluggable seam already exists (embedder.rs Embedder trait + FakeEmbedder).
