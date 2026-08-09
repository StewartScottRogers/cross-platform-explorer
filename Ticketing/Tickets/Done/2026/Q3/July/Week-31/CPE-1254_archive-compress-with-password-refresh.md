---
id: CPE-1254
title: "Bug: after Compress-with-password, the new archive row doesn't appear (listing not refreshed)"
type: Bug
priority: Low
component: Multiple
tags: [ready]
estimate: 1h
created: 2026-08-02
closed:
---

## Context
Surfaced (not caused) by the CPE-1249 gui-smoke gate. `archive-password.smoke.ts` test 2: "Compress with
password…" reports success (a "1 item compressed" toast fires) but the new archive row does not appear in
the listing afterward — the directory listing isn't refreshed on that path. Confirmed PRE-EXISTING — fails
identically on `main` baseline, unrelated to the vault work.

## What to do
Ensure the compress-with-password completion triggers a listing refresh (the `loadPath`/reload the plain
compress path presumably already does). Compare the with-password vs without-password completion handlers.
Re-confirm via the gui-smoke assertion.

## Done 2026-08-02 (sprint) — merged #555 @ 109e2f0a
Diagnosis corrected: not a missing refresh in the password path (both paths refresh identically) — a RACE in the shared transfer://done listener (fast op emits done before pendingArchiveOps is registered → silently dropped). Fix: unregistered clean compress/extract finish now refreshes + shows a fallback notice. Reviewer APPROVE; pinned by archive-password.smoke.ts test2 (reproduced-then-fixed, 2/2 real build).
