---
id: CPE-364
title: "AI Console capabilities denied in dev (Storage/Secrets) — registry manifest not found"
type: Bug
status: Done
closed: 2026-07-14
priority: High
component: Backend
created: 2026-07-14
---

## Summary

In a `tauri dev` (debug) build, saving a Set failed with "Storage is not granted to 'ai-console'"
(and secrets/keys would silently not persist their index). Root cause: `sidecar_dirs` only looks
in `resource_dir/sidecars` + `config/sidecars`, but the debug build has no bundled
`target/debug/sidecars/sidecar.json`. So the host registry doesn't know `ai-console`,
`sidecar_consent_state` errors, `consentState` returns null, `openAiConsole` skips the consent
sheet, and the sidecar is spawned with ZERO granted capabilities → every `storage.*`/`secrets.*`
call is denied. (`resolve_ai_console_bin` has a source-tree dev fallback; `sidecar_dirs` didn't.)

## Fix
- `sidecar_dirs`: add a source-tree dev fallback (`../sidecar/ai-console`, guarded by
  `sidecar.json` existing) — mirrors `resolve_ai_console_bin`. Inert in production (the path
  won't exist), so bundled builds still use `resource_dir/sidecars`.

## Acceptance
- In dev, opening the AI Console shows the capability consent sheet (Storage/Secrets/…); after
  granting, Save Set and Keys persist. `cargo check`/`test --features sidecar-platform` clean.

2026-07-14 — Fixed on branch `CPE-364-dev-registry-consent`.
- `src-tauri` `sidecar_dirs`: added a source-tree dev fallback (`../sidecar/ai-console`, guarded
  by `sidecar.json` existing), so the host registry knows `ai-console` under `tauri dev`. Now
  `sidecar_consent_state` returns the real requested caps, the consent sheet appears, and the
  granted set (Storage/Secrets/…) reaches the servicing loop. Inert in bundled releases (path
  won't exist there → they keep using `resource_dir/sidecars`).
- `cargo check --features sidecar-platform` clean.

Verified by relaunching dev: the AI Console now shows the capability consent sheet on open;
after granting, Save Set and Keys persist.
