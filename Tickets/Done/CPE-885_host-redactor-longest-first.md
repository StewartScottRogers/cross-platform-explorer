---
id: CPE-885
title: Host log/JSON Redactor must scrub overlapping secrets longest-first too
type: bug
component: Sidecar
priority: medium
tags: ready
epic: CPE-259
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
Sibling of CPE-884 in a **second, independent redactor**: `sidecar-host::observability::Redactor::redact_str`
(which backs `redact_json` and `redact_log_line`, i.e. the `sidecar_diagnostics` bundle) replaced registered
secrets in **insertion order**. A shorter registered secret that is a substring of a longer one rewrote part
of the longer secret before it could be matched whole, leaking the remaining fragment (redacting `"SECRET"`
out of `"SECRETLONGER"` left `"LONGER"` in the diagnostics output/log).

Fix: redact the longest registered secrets first, matching the CPE-884 fix in the ai-console redactor.

## Acceptance Criteria
- [x] `Redactor::with_secret("SECRET").with_secret("SECRETLONGER").redact_str("token=SECRETLONGER")` →
      `"token=***"` (no `LONGER` leak), regardless of registration order.
- [x] The heuristic `redact_secret_patterns` / JSON-key / assignment scrubbing is unchanged (audited sound).
- [x] `sidecar-host` observability tests (9) + `cargo clippy --all-targets -D warnings` green.

## Work Log
- 2026-07-22 (autonomous) — After CPE-884, grepped for other redactors and found the host's had the same
  order-dependent substring leak. Applied the same longest-first sort; added a matching regression test.
  9/9 observability tests pass; clippy clean.
