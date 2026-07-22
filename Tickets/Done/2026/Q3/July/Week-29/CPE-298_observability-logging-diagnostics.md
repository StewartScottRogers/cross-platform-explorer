---
id: CPE-298
title: "Observability: logging, tracing & diagnostics export"
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

A huge, always-growing system is only maintainable if failures are diagnosable.
Give the platform structured, correlated logging across the host↔sidecar boundary,
per-sidecar log capture, and a one-click "export diagnostics" bundle — with secrets
redacted everywhere ([[CPE-268]]).

## Acceptance Criteria

- [ ] Structured logs with a correlation id spanning a host request → sidecar →
      response; per-sidecar stdout/stderr captured.
- [ ] Log levels configurable per sidecar; a viewer in the management UI ([[CPE-274]]).
- [ ] "Export diagnostics" produces a shareable bundle (versions, manifests, recent
      logs, health) with all secret values redacted.
- [ ] Redaction is a shared, tested utility used by every log/transcript path.

## Resolution

Implemented `observability` in `sidecar-host`: the shared **`Redactor`** (scrubs
registered secret values from any string, blanks values under a `SENSITIVE_KEYS` list
in JSON, recursively) — the single redaction utility every log/transcript/diagnostics
path uses (referenced by [[CPE-268]], [[CPE-292]]); `LogRecord` (correlation_id +
sidecar_id + level + message) with a bounded, thread-safe `LogCapture` ring buffer; and
`Diagnostics` + `build_diagnostics` which assemble a shareable bundle with all log
messages run through the redactor. 5 tests (string + JSON redaction, empty-secret
ignored, bounded ring order, diagnostics redaction). 49 unit + 3 E2E + clippy green.

**Deferred to the mgmt UI ([[CPE-274]]):** the in-app log viewer and per-sidecar
configurable levels (data model is ready here).

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-13 — Implemented during dayshift (in parallel with the CPE-273 sub-agent). Done.
