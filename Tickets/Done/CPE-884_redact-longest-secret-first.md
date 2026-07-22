---
id: CPE-884
title: Redact overlapping secrets longest-first so no secret fragment leaks to disk
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
`ai-console::history::redact` (the transcript scrubber that keeps injected API keys off disk) replaced each
secret in **array order**. If a shorter secret is a substring of a longer one, the shorter replacement
rewrites part of the longer secret before it can be matched as a whole, leaking the remainder: redacting
`"SECRET"` out of `"SECRETLONGER"` yields `"***LONGER"` — the fragment `LONGER` (part of a real secret
value) then persists to storage.

Fix: redact the **longest** secrets first (standard longest-match-first for multi-pattern redaction), so a
longer secret is fully replaced before any of its substrings can mangle it.

## Bug
- Before: overlapping secrets could leave a fragment of a longer key in the stored transcript.
- After: the longer secret is redacted whole; no fragment survives, regardless of input order.

## Acceptance Criteria
- [x] `redact("token=SECRETLONGER", ["SECRET","SECRETLONGER"])` → `"token=***"` (no `LONGER` leak),
      independent of the order secrets are passed in.
- [x] Existing redaction, capping, rotation, JSON round-trip behavior unchanged.
- [x] `ai-console` history tests + `cargo clippy --all-targets -D warnings` green.

## Work Log
- 2026-07-22 (autonomous) — Found the substring-overlap leak while auditing the sidecar's secret redactor.
  Sorted secrets longest-first; added a regression test (passes secrets shortest-first, so it fails against
  the old order-dependent code). 7/7 history tests pass; clippy clean.
