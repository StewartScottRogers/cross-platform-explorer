---
id: CPE-346
title: "AI Console launcher: provider credential management UI"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Frontend
created: 2026-07-13
---

## Summary

Final CPE-287 sub-ticket: the launcher UI to manage provider keys on the CPE-345 API. A
"Keys" panel to add a key (masked input), pre-check its format, save it to the keychain,
see which providers have a stored key, and remove one. Values are never re-displayed.

## Acceptance
- Add/verify/save a provider key; masked input; value never shown after save (input cleared).
- Panel lists providers that have a stored key with a Remove action.
- Ships in the embedded launcher (cargo build). Backend already tested (CPE-345).

## Work Log
2026-07-13 (Dayshift) — Filed as the CPE-287 UI layer.

2026-07-13 (Dayshift) — Implemented in `launcher.html`. A "Keys…" toolbar button opens a
credential panel: provider dropdown (union of catalog providers), masked (`type=password`)
key input, **Check** (calls `/api/keys/verify` — offline format pre-check), **Save** (POSTs
`/api/keys`, then clears the input so the value is never retained), a list of providers that
have a stored key (`GET /api/keys`, names only) each with **Remove** (`/api/keys/delete`).
Closes on ×, backdrop click. `cargo build` embeds it clean; 98 lib + 7 integration still pass.

VISUAL QA PENDING: launcher UI can't be driven headlessly — recommend an eyeball (open the AI
Console → Keys…). The API beneath is fully unit-tested (CPE-345).
