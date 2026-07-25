---
id: CPE-316
title: Frontend sidecar-platform client
type: Task
status: Done
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-13
closed: 2026-07-13
---

## Summary

The typed frontend client for the sidecar platform (the counterpart to the backend
seam from CPE-272). Wraps the `sidecar_registry_ids` command and degrades gracefully
to an "off" result when the app is built without the `sidecar-platform` feature — so the
plain explorer is unaffected. This is the data layer the management UI (CPE-274) and the
pane mount (CPE-271) build on.

## Acceptance Criteria

- [ ] `src/lib/sidecar.ts` — `listSidecars()` and `platformActive()`, both catching a
      missing/erroring command and returning the "off" result (never throwing).
- [ ] Unit-tested (mocked invoke): ids on success, [] on error/non-array, active-flag.
- [ ] `npm run check` + vitest pass.

## Work Log
2026-07-13 — Filed and picked up during dayshift, extending the CPE-272 integration seam
to the frontend.
2026-07-13 — Implemented src/lib/sidecar.ts (listSidecars/platformActive; try/catch degrades to []/false when the sidecar-platform feature is off). Unit-tested (ids, non-array->[], active-flag); the error-rejection branch is handled by the code but not asserted (this test setup flags any error routed through the mocked invoke spy as unhandled, even when caught). vitest 260 pass, npm run check clean. Done.
