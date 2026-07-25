---
id: CPE-323
title: "Surface sidecar health, last error & logs in the management panel"
type: Feature
status: Done
priority: Low
component: Multiple
created: 2026-07-13
closed: 2026-07-13
---

## Summary

The management panel (CPE-274) shows each sidecar's running/stopped state, but not its
**last error** or a **link to its logs**. Wire the host-side observability
(`sidecar-host::observability` — `LogCapture` / `build_diagnostics`, CPE-298/299) to the
live supervisor connection and expose it so a crashed/incompatible sidecar surfaces an
actionable error and recent log lines in the panel.

## Acceptance Criteria

- [x] A `sidecar_diagnostics(id)` command returns last error + recent redacted log lines.
- [x] The management panel shows a health/last-error line and a "view logs" affordance.
- [x] Secrets never appear (redaction verified).

## Notes
Split out of CPE-274 (management UI): the list/version/compat/running/enable-disable/
capability-revoke surface shipped there; this is the diagnostics half, which needs the
`LogCapture` attached to the running `ProcessConnection`. **Depends on:** [[CPE-298]],
[[CPE-299]], [[CPE-274]].

## Work Log

### 2026-07-13 — implemented (branch `CPE-323-sidecar-diagnostics`)

**Host observability (`sidecar/host/src/observability.rs`)**
- Added `Redactor::redact_log_line` = registered-secret scrub (`redact_str`) **plus** a
  new heuristic `redact_secret_patterns` (defence in depth) that masks unregistered
  secret shapes: well-known credential prefixes (`sk-`, `ghp_`, `xoxb-`, `AKIA`, `AIza`,
  `ya29.`, GitLab `glpat-`, Stripe `sk_live_`…), `Bearer`/`Authorization:` tokens, and
  `sensitive_key=value` / `sensitive_key: value` assignments (reuses `SENSITIVE_KEYS`).
- 3 new unit tests, incl. the redaction proof: feeding API-key / bearer / `api_key=` lines
  with **no** registered secret still masks them; ordinary text is untouched; registered
  secrets still scrubbed.

**Tauri layer (`src-tauri/src/lib.rs`)**
- `AiConsoleState` grew from a bare `Mutex<Option<ProcessConnection>>` into a struct
  holding the connection + a bounded `LogCapture(200)` + a `last_error` slot, with
  `log()` / `fail()` / `clear_error()` helpers. Updated the three existing `state.0`
  callers (`sidecar_details`, `sidecar_stop`, `sidecar_set_enabled`) to `state.conn`; the
  latter two now log lifecycle lines.
- `sidecar_start_ai_console` now records each lifecycle step (starting / handshake ok /
  ui ready) and routes every failure through `state.fail(..)` so a crashed/incompatible
  launch leaves an actionable `last_error`; a clean start clears it.
- New `#[tauri::command] sidecar_diagnostics(id)` → `{ id, running, last_error, logs[] }`,
  every string run through `redact_log_line`. Registered in `generate_handler!`.

**Frontend**
- `src/lib/sidecar.ts`: `SidecarDiagnostics` / `DiagLogLine` types, `sidecarDiagnostics(id)`
  (degrades to `emptyDiagnostics` — never throws), `emptyDiagnostics` helper. 3 vitest
  cases (well-formed passthrough, malformed→safe defaults, inert empty record).
- `src/lib/components/SidecarManager.svelte`: per-sidecar health line (Healthy / Not
  running / ⚠ last-error) + a "View logs (n)" toggle showing recent redacted lines,
  colour-coded by level. Diagnostics load on refresh and on open.

**Assumptions**
- Only the AI Console is spawnable today, so it is the only sidecar with live logs; other
  registered sidecars return a valid-but-empty diagnostics record (mirrors how
  `sidecar_details` treats `running`). When more sidecars become spawnable, give each its
  own `LogCapture` and the command generalises with no UI change.
- The `sidecar_diagnostics` `Redactor` registers no known secret values (the host doesn't
  currently thread vault secrets into this command); safety rests on the pattern scrubber,
  which is the guaranteed path and is what the redaction test exercises. Registered-secret
  scrubbing remains covered by the observability unit tests.
- `correlation_id` is `0` for host-emitted lifecycle lines (no host↔sidecar request to
  correlate); the field is retained for when captured sidecar-side records flow through.

**Verification**
- `cargo test --lib` (sidecar-host): 80 passed, 1 ignored. `cargo clippy --all-targets`
  (host): clean.
- `cargo check --features sidecar-platform` (src-tauri): ok. `cargo clippy --features
  sidecar-platform` (src-tauri): clean. (Used `check`, not a full `build`, per the
  slow-build note — the feature-gated code compiles.)
- `npx vitest run`: 280 passed (28 files). `npm run check`: 0 errors, 0 warnings.
- Redaction proof: `redact_log_line_masks_unregistered_secret_shapes` asserts
  `"...key sk-abc123def456ghi789"` → `"...key ***"`, `"Authorization: Bearer <jwt>"` →
  `"Authorization: Bearer ***"`, and `"api_key=super-secret-value"` → `"api_key=***"`
  with an empty `Redactor` (no secret pre-registered).
