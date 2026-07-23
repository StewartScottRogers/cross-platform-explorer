---
id: CPE-953
title: tauri-specta typed bindings — full command/type rollout + runtime Builder + GUI verify
type: feature
component: Multiple
priority: medium
status: Open
tags: ready
created: 2026-07-23
epic: CPE-810
estimate: 4h+
---

## Summary
Follow-up to **CPE-812**, which landed the codegen **foundation**: the `specta-bindings` feature, the
`export_bindings` bin, the busy-cursor-routed `src/lib/bindings.gen.ts`, and a proving set of 6 commands.
This ticket completes the rollout so the whole app can call backend commands through the typed client.

## Acceptance Criteria
- [ ] `#[cfg_attr(feature = "specta-bindings", specta::specta)]` on **all** `#[tauri::command]` fns; `Type`
      derived (feature-gated) on **every** serde type they use — including the `cpe-server` types (add an
      optional `specta` dep + a `bindings`/`typescript` feature to `cpe-server`, keeping it OFF by default so
      the lean crate is unchanged in normal builds).
- [ ] The 3 `ipc::Channel` streamers are typed, not raw; streaming call sites keep using `rawInvoke` /
      `createChannel` (CPE-547/550), not the busy-cursor path.
- [ ] Codegen runs with **`sidecar-platform` ON** too, so the feature-gated commands/types get types (one
      superset `bindings.gen.ts`); document the regen command set.
- [ ] Optionally switch runtime dispatch from `generate_handler!` to the specta `Builder`'s
      `invoke_handler()` (only if it stays byte-for-byte; otherwise keep `generate_handler!` and just emit).
- [ ] Migrate a first batch of call sites to import from `bindings.gen.ts`; `npm run check` + vitest green.
- [ ] GUI-verified: app launches and a few commands (e.g. `list_dir`, `board_cards`) work through the
      generated typed client.

## Notes
Foundation reference (all in CPE-812): the loader failure is dodged by keeping tauri-specta in the plain
`export_bindings` bin (never a libtest binary) and gating everything behind the OFF-by-default
`specta-bindings` feature. Regenerate with `cargo run --bin export_bindings --features specta-bindings` and
commit `src/lib/bindings.gen.ts`. Watch the `u64`→`number` BigInt policy and the `@ts-nocheck` header the
generator adds. CPE-824's dispatch registry is the natural convergence point for the typed method surface.
