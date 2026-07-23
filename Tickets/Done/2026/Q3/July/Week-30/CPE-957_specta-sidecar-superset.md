---
id: CPE-957
title: tauri-specta superset codegen — type the sidecar-platform commands too
type: feature
component: Multiple
priority: low
status: Done
tags: ready
created: 2026-07-23
closed: 2026-07-23
epic: CPE-810
estimate: 2-3h
---

## Summary
Split from **CPE-953**, which typed all 92 non-sidecar commands. This ticket types the ~33
`sidecar-platform`-gated commands too, so `bindings.gen.ts` is one **superset** built with both
`specta-bindings` + `sidecar-platform`.

## What it needs (from the CPE-953 spike)
The sidecar command return types chain `specta::Type` across crates:
- `sidecar-contract` — `Capability` etc. (a proven bulk `#[cfg_attr(feature="specta", derive(Type))]` works;
  exclude the wire-envelope chain `Request`/`Response`/`Message`/`Envelope` — `Response.result` is a
  `Result<Value,_>` field specta can't represent, and they aren't command types). Add an optional `specta`
  dep (`features = ["derive","serde_json"]`) + a `specta` feature.
- `repos` — `RepoEntry` (and any other types the `forge_*` commands return): same optional-specta pattern.
- `sidecar-host` — check whatever remaining sidecar command types resolve through it.

Then: the app's `specta-bindings` feature enables each via the weak-dep syntax (`sidecar-contract?/specta`,
`repos?/specta`, …); gate `export_bindings` + the bin on **both** features (`#[cfg(all(feature =
"specta-bindings", feature = "sidecar-platform"))]`, `required-features = ["specta-bindings",
"sidecar-platform"]`); add the sidecar command names unconditionally to `collect_commands!` (the macro
rejects `#[cfg]` entries, so the whole export must be dual-feature). Regenerate with
`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`.

## Acceptance Criteria
- [x] `specta::Type` (feature-gated) on the sidecar command types: `sidecar-contract` (optional `specta`
      feature w/ `derive`+`serde_json`; bulk-derived, excluding the wire-envelope chain
      `Request`/`Response`/`Message`/`Envelope` — `Response.result: Result<Value,_>` isn't representable and
      they aren't command types) + `repos` (optional `specta` feature, pulls `sidecar-contract/specta`) +
      the app modules (`forge_egress.rs` `RepoEntry` etc.). No `sidecar-host` changes were needed.
- [x] `export_bindings` gated on **both** `specta-bindings`+`sidecar-platform` (bin `required-features`);
      app `specta-bindings` enables the crate specta via weak-deps (`sidecar-contract?/specta`,
      `repos?/specta`); sidecar commands added unconditionally to `collect_commands!`. Superset
      `bindings.gen.ts` now types **125 commands** (92 non-sidecar + 33 sidecar).
- [x] Default builds/`cargo test` never compile specta (all OFF by default): default + `sidecar-platform`
      builds clean; default `cargo test` 67 pass + loads clean; clippy clean default + `sidecar-platform` +
      dual-feature bin; `npm run check` 0/0.

## Resolution
Superset shipped. Regenerate with `cargo run --bin export_bindings --features "specta-bindings
sidecar-platform"`. The sidecar crates each carry an OFF-by-default `specta` feature, so normal sidecar
builds are byte-for-byte unchanged. Completes the CPE-953 arc: the whole 125-command surface is now typed.
