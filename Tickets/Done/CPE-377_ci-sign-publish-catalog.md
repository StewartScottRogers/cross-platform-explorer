---
id: CPE-377
title: "CI: build, sign & publish the catalog bundle as release assets (CPE-308 part 2, slice 2d)"
type: Task
status: Done
priority: Medium
component: CI
tags: [ready]
estimate: 2h
created: 2026-07-14
closed: 2026-07-14
---

## Summary

Produce the signed catalog bundle at release time and upload it next to the installer, plus embed
the trusted public key and document the keypair. This is the release-side counterpart to the fetch
(CPE-376).

## Acceptance Criteria

- [x] A script builds `catalog-index.json` from the bundled `ai-console/agents/*.json`
      (id, schema_version, sha256, monotonic version) and signs the index + each manifest (ed25519)
      with a CI secret key.
- [x] The release workflow uploads `catalog-index.json` + `*.sig` + the manifests as release assets
      (alongside the installer).
- [x] The trusted **public** key is embedded in the app config (a constant / config file), consumed
      by CPE-376 and passed to the sidecar.
- [x] README "Agent catalog updates" section: how it works + how to generate the keypair and set the
      `CPE_CATALOG_SIGNING_KEY` CI secret (mirrors the updater-key docs).

## Notes — key is the user's step
The signing **keypair** must be generated once and the private key added as a CI secret (like the
updater key) — an operator action, not code. I ship the script + workflow + a **placeholder pubkey**;
it goes live when the real key is dropped in. Part of [[CPE-308]]; pairs with [[CPE-376]].

## Work Log
2026-07-14 — Filed.
