---
id: CPE-276
title: Sidecar packaging (bundled-only, no code-signing needed)
type: Feature
status: Done
priority: Medium
component: Packaging
estimate: 4h+
created: 2026-07-13
closed: 2026-07-13
---

## Summary

How sidecar binaries ship, are **code-signed**, and update/rollback on their **own
cadence** — the point of "keep up with the market without ricochet." A sidecar can
be bundled with the app or fetched/updated independently of the explorer's release,
without breaking the delete-test or the explorer's own updater pipeline.

## Acceptance Criteria

- [ ] Defined layout for bundled sidecars + an install location for
      fetched-at-runtime sidecars.
- [ ] Sidecar binaries are **code-signed** (Windows/macOS) so they don't trip
      SmartScreen/Gatekeeper; signature verified before launch (ties to [[CPE-002]]
      code-signing and [[CPE-295]] trust).
- [ ] A sidecar can **update** to a new compatible-contract version without an
      explorer release; incompatible versions refused cleanly ([[CPE-263]]).
- [ ] **Rollback**: pin/restore a previous sidecar version; a failed update reverts
      to last-known-good rather than leaving a broken tenant.
- [ ] Contract-**major** bumps are coordinated (host advertises supported range;
      sidecars that need the new host are held until the host ships).
- [ ] Uninstall removes the binary, its storage ([[CPE-269]]), secrets scope, and
      registry entry.
- [ ] Does not disturb the explorer's own updater/latest.json flow.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-264]], [[CPE-265]], [[CPE-263]]. **Phase:** P5.
**Epic:** [[CPE-260]]. Related: [[CPE-300]] (migration), [[CPE-308]] (catalog updates).

## Resolution

**Decision (user, 2026-07-13): sidecars are first-party and always BUNDLED with the app, NEVER downloaded at runtime.** Recorded in ADR 0001. This removes the fetched-binary path and its whole tail: no OS code-signing of sidecars (they inherit the signed app's trust — app signing is [[CPE-002]]), no independent download/update/rollback of binaries (a sidecar updates only when the app updates). Integrity of the bundled content is still covered by ed25519 manifest signing ([[CPE-295]]).

**Delivered:** a config overlay `src-tauri/tauri.sidecar.conf.json` that bundles the ai-console release binary + its `sidecar.json` manifest + the agent catalog into the app's `sidecars/` resource dir, merged at build time so the DEFAULT app stays sidecar-free (delete-test): `tauri build --features sidecar-platform --config src-tauri/tauri.sidecar.conf.json` (build the sidecar release first). Added `sidecar/ai-console/sidecar.json` so the registry lists it. `resolve_ai_console_bin` already prefers the resource dir, so an installed feature build finds the bundled binary with no env var (dev keeps the `CPE_AICONSOLE_BIN`/dev-tree fallbacks). Verified the resource sources exist + a bundle build.

**Obsolete under this decision (closed by design):** runtime-download install location, sidecar binary update/rollback, coordinated contract-major *download* upgrades. **Follow-up:** update `release.yml` to build the sidecar + use the overlay for a sidecar-enabled release channel; macOS/Linux overlay entries (per-OS binary names).

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Hardened: added binary code-signing, rollback / last-known-good, and
coordinated contract-major upgrades.
2026-07-13 — Adopted bundled-only sidecars; added the bundle overlay + sidecar manifest; rescoped away code-signing. Done.
