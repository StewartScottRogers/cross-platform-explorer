---
id: CPE-1524
title: "Gate the ＋Add action on discovered rows whose scheme isn't savable yet (e.g. mDNS nfs://)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-1517
created: 2026-08-09
---
## Why (opus review of PR #743 / CPE-1523, non-blocking nit)
mDNS discovery browses `_nfs._tcp` and can surface an `nfs://host` row in the "Discovered on your network"
tier. But `nfs` is NOT in `SUPPORTED_SCHEMES` (no NFS provider yet — CPE-1505 unbuilt), so clicking **＋ Add a
connection** on such a row prefills scheme `nfs` and then fails `buildConnection` with "Unsupported protocol
nfs". A small UX papercut: the row looks actionable but errors on click. (Documented as an accepted limitation
in `parseSchemeAuthority`'s doc comment; matches the tier's current design, so it was not a merge blocker.)

## Scope (small, frontend-only)
- For a discovered row whose derived scheme is **not in `SUPPORTED_SCHEMES`** (currently `nfs`, and any future
  discovered-but-not-yet-savable scheme), either:
  - **(preferred)** disable / hide the ＋Add affordance on that row (with a tooltip like "NFS isn't supported
    yet"), so it's visibly informational-only, **or**
  - keep it clickable but show the "not supported yet" message inline instead of a generic validation error.
- Keep the row itself **visible** (discovery is still useful — you can see the host exists); only the *add*
  action is gated.
- Pure logic (an `isSavableScheme(scheme)` helper against `SUPPORTED_SCHEMES`) → unit-test in `network.test.ts`.
- When a provider later lands (e.g. NFS via CPE-1505, or SMB via CPE-1504), that scheme joins `SUPPORTED_SCHEMES`
  and the gate opens automatically — no per-scheme special-casing.

## Verify
- Unit test: a discovered `nfs://` row is flagged not-savable; an `sftp`/`webdav`/`ftp`/`smb` row is savable.
- `npm run check` + vitest green. Visual sign-off (the disabled/hidden affordance) folds into the owed sidebar
  visual review.

## Notes
Trivial follow-up to the merged CPE-1523 (mDNS discovery). Same epic (CPE-1517). Good small batched-run ticket.
