---
id: CPE-1389
title: "Test: jsdom render-spec for IntegrityDialog (checksum/verify/bitrot) — retires an MVD row"
type: Task
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-737
created: 2026-08-07
---

## Problem (QA-Architecture MVD burndown)
`src/lib/components/IntegrityDialog.svelte` (CPE-792, bitrot/integrity UI) ships but has ZERO jsdom coverage —
verifiable today only by an attended build→run. It even ships unused `data-testid` hooks
(`baseline-btn`/`verify-btn`/`counts`/`group-*`/`all-ok`) — a spec was intended and never written.

## Fix direction
Add `src/lib/components/IntegrityDialog.test.ts` using the established recipe (`vi.mock("../bindings.gen")` /
`vi.mock("@tauri-apps/api/core")` + `@testing-library/svelte` render/fireEvent, as `DuplicatesDialog.test.ts`
models). Assert: Baseline btn → `commands.checksumFolder(path)` + dispatches `baseline`; Verify →
`commands.verifyFolder(path, baseline)`; renders corrupted/missing/edited/new/intact counts + grouped lists;
`all-ok` when clean; error + "no baseline yet" states; startup-toggle → `setVerifyOnStartup`. Assert BOTH the
typed-command call args AND dispatched event payloads. If the component mis-wires anything, REPORT it (don't
fix — separate ticket). Test-only; parallel-safe.
