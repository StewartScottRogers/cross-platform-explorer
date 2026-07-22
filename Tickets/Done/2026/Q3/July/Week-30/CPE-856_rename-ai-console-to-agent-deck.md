---
id: CPE-856
title: Rename the "AI Console" to "Agent Deck" (user-facing)
type: chore
component: Multiple
priority: low
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
"AI Console" was the only agent surface not in the "Agent …" family (Agent Board / Agent Watch / Agent
Grid). Rename the **user-facing** product name to **Agent Deck** everywhere it's shown — the sidecar
manifest name, all 12 i18n locales, toolbar/menu labels, window title, notices, and the docs. Keep the
**internal id** `ai-console` (manifest id, window label, capabilities, bundle paths, `CPE_AICONSOLE_*` env
vars, consent/enablement key, crate dir, Rust/TS symbols) unchanged — renaming those is a risky,
runtime-only-verifiable change that orphans stored consent for no user benefit.

## Acceptance Criteria
- [x] Every user-visible "AI Console" reads "Agent Deck": sidecar `name`, all locale values, labels, window
      title, notices, in-app + repo docs.
- [x] The internal id `ai-console` and its plumbing are unchanged (the sidecar still launches/handshakes).
- [x] `npm run check` clean; full frontend suite green (test expectations updated); `cargo clippy
      --all-targets -D warnings` clean in both feature modes.

## Work Log
- 2026-07-21 — Picked up. Global replace of the display string + per-locale i18n fixes; keep the id.

## Resolution
Renamed the user-facing product name **"AI Console" → "Agent Deck"** everywhere it's shown, keeping the
internal id `ai-console` and all its plumbing.

**Renamed (user-visible):**
- `sidecar/ai-console/sidecar.json` `name` → "Agent Deck" (what SidecarManager shows).
- `src/lib/i18n.ts` — all 12 locales: `tb.aiConsole` → "Agent Deck", `palette.openAiConsole` → localized
  "Open Agent Deck" (verb kept per locale, product name in Latin like the AI Console was for most).
- `src/App.svelte` + components (Sidebar, AgentMenu, BoardView, RepoBrowser) + `src/lib/*.ts` — window
  title, notices, menu labels, the docs-section label, comments.
- The sidecar's own served UI (`sidecar/ai-console/src/launcher.html`, `ui.rs`, console/session strings) +
  its docs, and cross-referencing comments in the `agent-board`/`repos`/`host` crates + `sidecar/README.md`.
- In-app docs (`src/docs/*`) + repo docs (`docs/**`, `docs/index.html`).
- Test expectations updated to match (AgentMenu, ai-console-launcher, docs, AgentMenu regex).

**Kept (internal, unchanged) — no user benefit, high churn/risk:** the id `ai-console` (manifest id, Hello
handshake, registry/consent/enablement key), the crate dir `sidecar/ai-console`, the window label
`ai-console`, capabilities, bundle paths (`sidecars/ai-console.exe`), `CPE_AICONSOLE_*` env vars, the
session daemon, and code symbols (`AiConsoleState`, `sidecar_start_ai_console`, `startAiConsole`,
`AI_CONSOLE_LABEL`). So the sidecar still launches/handshakes/bundles exactly as before.

Verification: **0** residual "AI Console"/"ai console" (the id `ai-console` intentionally remains); `npm run
check` → 0/0; full frontend suite → **902 passed** (expectations updated); `cargo clippy --all-targets -D
warnings` clean on the touched sidecar crates (ai-console, repos, agent-board). The visible name flip
confirms on the next build; the running app is unaffected structurally.

## Work Log
- 2026-07-21 — Global display-string rename + per-locale i18n fixes + sidecar served-UI strings + docs;
  kept the internal id. Fixed test expectations (incl. a lowercase `ai console` regex the exact-case sed
  missed). 902 tests + clippy clean; 0 residuals. Closing.
