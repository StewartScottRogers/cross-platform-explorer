---
id: CPE-1524
title: "Gate the ＋Add action on discovered rows whose scheme isn't savable yet (e.g. mDNS nfs://)"
type: Task
status: Done
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

## Work Log
- 2026-08-09: **Landed** (PR pending). `network.ts` gained `isSavableScheme(scheme)` — a pure, case/whitespace-
  tolerant check against `SUPPORTED_SCHEMES` — and `buildConnection`'s own scheme-validation branch now calls it
  too (single source of truth, no duplicated logic). `Sidebar.svelte`'s "Discovered on your network" tier
  (the `dedupedDiscovered` `{#each}`) computes `prefill`/`savable` once per row via `{@const}`, then: sets the
  row button's native `disabled` attribute (picks up the existing global `button:disabled { opacity: 0.38 }`
  from `app.css`, the same treatment the Explore section's "Gallery — not implemented yet" row already uses —
  no new CSS needed), swaps the tooltip to `"<SCHEME> isn't supported yet"` when not savable, hides the
  "＋" hint icon, and guards the `on:click` handler with `savable &&` (belt-and-suspenders: a jsdom
  `fireEvent.click` bypasses the native `disabled` gate in the test harness, so the explicit guard is what
  actually blocks the dispatch in both the real browser and the test). The row itself stays visible/rendered
  either way — only the add affordance gates, per the ticket's "keep discovery useful" requirement. No
  per-scheme special-casing: the gate is entirely `SUPPORTED_SCHEMES`-driven, so a future NFS (CPE-1505) or SMB
  (CPE-1504) provider landing opens it automatically.
  Tests: `network.test.ts` gained a new `isSavableScheme` describe block (nfs → false; sftp/webdav/ftp/smb →
  true; case/whitespace tolerance) — 6 new assertions across 3 tests. `Sidebar.test.ts` gained two component
  tests in the existing "Discovered on your network tier" describe: an `nfs://` mDNS row renders visible +
  `disabled` + the "NFS isn't supported yet" tooltip and does NOT dispatch `networkAdd` on click; a savable
  `sftp://` mDNS row stays enabled and DOES dispatch `networkAdd` with the expected prefill. No tests removed
  (nothing existing was obsoleted).
  **Verify, all green:** `npm run check` (svelte-check) 0 errors/0 warnings. `npx vitest run` on
  `src/lib/network.test.ts` (54/54), `src/lib/components/Sidebar.test.ts` (26/26, was 24 — the 2 new tests),
  `src/lib/components/Sidebar.hoverSameVolume.test.ts` (3/3) — no regressions.
  **Still owed:** the ticket's Verify section notes visual sign-off "folds into the owed sidebar visual
  review" — this pass is headless-only (no attended GUI check of the actual dimmed/disabled row rendering).
