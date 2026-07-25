---
id: CPE-758
title: Diagnostics mode — Application-menu toggle + on-screen timing of every OS call
type: feature
component: Frontend
tags: ready
created: 2026-07-19
closed: 2026-07-19
status: Done
priority: medium
estimate: 2-3h
---

## Summary
The temporary nav perf readout (CPE-757) was useful, so make it a real, permanent **Diagnostics mode**:
a persisted **Application → Diagnostics** toggle that shows/hides an on-screen readout timing **every**
backend/OS resource call — not just navigation.

## Design (instrument the one chokepoint)
All OS access goes through the shared `invoke` / `rawInvoke` wrapper (`src/lib/invoke.ts`), so timing
*that* covers everything — directory listings, disk space, git status, dir sizes, checksums, previews,
search, archive reads, thumbnails — and any future OS call is covered by construction.

- `src/lib/diagnostics.ts` (new): a bounded store of recent `{cmd, ms, ok}` calls; `recordCall` (no-op
  when off → zero overhead), `setDiagnosticsEnabled`, `diagnosticsEnabled`.
- `src/lib/invoke.ts`: both `invoke` and `rawInvoke` now time the call and `recordCall` when Diagnostics
  is on.
- `DiagnosticsOverlay.svelte` (new): bottom-left panel of recent calls, colour-banded by duration
  (green/amber/red) + the slowest call; `pointer-events:none`.
- Setting `cpe.diagnostics` (settings.ts) — persisted, off by default. Force on from a console via
  `localStorage["cpe.diagnostics"] = "true"`.
- `MenuBar`: Application → **Diagnostics** toggle item (activity icon + ✓ when on); items can now carry a
  plain `label` (no i18n) so a dev toggle doesn't need the 12-locale gate.
- App wires the setting → `setDiagnosticsEnabled`, passes it to `MenuBar`, renders the overlay when on.
  Removed the CPE-757 ad-hoc nav timing (the general instrumentation supersedes it).

## Acceptance
- [ ] Application → Diagnostics toggles the overlay on/off (with a ✓ when on) and persists across restarts.
- [ ] The overlay lists recent OS calls with durations; slow ones stand out.
- [ ] It covers ALL OS calls (listing, disk, git, dir-size, preview, search, …) automatically via `invoke`.
- [ ] Zero overhead when off; `npm run check` clean; test suite passes.

## Notes
Standing rule recorded in memory: route OS calls through `lib/invoke` so they're auto-diagnosed. This is a
first-class feature now, not a temporary diagnostic.
