---
id: CPE-471
title: "Unified reseller manifest (launch + models + egress + auth); migrate the existing 9"
type: Feature
status: Done
priority: High
component: Sidecar (AI Console)
tags: [ready]
estimate: 2-3h
created: 2026-07-15
closed: 2026-07-16
epic: CPE-467
---

## Summary
Collapse the split reseller data (model-list manifest + provider recipe + egress) into ONE reseller
manifest describing launch descriptor, model-list endpoint, egress hosts, and auth. Migrate the 9
existing `resellers/*.json` to the unified schema.

## Acceptance Criteria
- [x] One `resellers/<id>.json` schema now carries launch + list + egress + auth: extended the
      existing `ResellerManifest` with optional `protocol` + `launch_base_url` (alongside
      `models_endpoint`, `api_hosts`, `auth`, `web_base`).
- [x] Existing resellers migrate with **no behaviour change** — the new fields are additive/optional,
      the model-list path is untouched. 6 OpenAI-compatible resellers gained launch fields; the model
      list still resolves for all.
- [x] Loader + validation: `protocol` (anthropic|openai) + https `launch_base_url` validated;
      unknown/absent fields ignored via `#[serde(default)]` (forward-compatible, schema_version kept).
- [x] Tests: descriptor derivation (present vs absent), `descriptors()` filtering, malformed-launch
      rejection, and a bundled-resellers test asserting the 6 migrated ones are launch-capable.

## Resolution
Made the reseller manifest **unified** by extending it (rather than a parallel schema), so one file
drives both model-listing and launch:
- `ResellerManifest` gains optional `protocol` (`anthropic`|`openai`) + `launch_base_url`
  (`{base_url}` for `compose_reseller_launch`, CPE-468), validated (known protocol, https URL).
- `ResellerManifest::descriptor()` → `Option<ResellerDescriptor>` (Some when launch-capable);
  `ResellerRegistry::descriptors()` → the launch-capable set, id-sorted (what the launcher will offer).
- **Migrated 6** OpenAI-compatible resellers (`together`, `fireworks`, `groq`, `deepinfra`, `novita`,
  `aimlapi`) with `protocol:"openai"` + their `/v1` base URLs. Left `openrouter` (keeps its existing
  anthropic `provider_recipe` path), `github-models`, and `wavespeed` as model-list-only for now —
  migrating openrouter to a descriptor + multi-protocol is a finer point for a later step.
- **3 new tests** (191 lib tests pass); `cargo clippy --all-targets -D warnings` clean.

Next: CPE-469 wires `descriptors()` into the launcher provider dropdown + routes launches through
`compose_reseller_launch` (needs agents to declare an `openai` `reseller_recipes` entry), and CPE-470
derives the egress allow-list from the reseller hosts. Nightshift loop 3.
