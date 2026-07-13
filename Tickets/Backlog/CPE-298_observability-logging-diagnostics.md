---
id: CPE-298
title: "Observability: logging, tracing & diagnostics export"
type: Feature
status: Open
priority: Medium
component: Multiple
estimate: 3-4h
created: 2026-07-13
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

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-265]], [[CPE-262]]. **Phase:** P2. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
