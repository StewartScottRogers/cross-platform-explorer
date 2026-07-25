---
id: CPE-862
title: Sidecar reliability & self-healing
type: epic
status: In Progress
priority: high
tags: reliability
created: 2026-07-21
---

## Goal
Make the out-of-process sidecar platform (Agent Deck / Agent Board / Repositories) **trustworthy**: a
sidecar that is missing, stale, disabled, or crashed should be *detected*, *explained in plain language*,
and — where possible — *repaired automatically*, instead of collapsing into a vague "isn't available"
notice with no way to diagnose.

## Why (observed pain, 2026-07-21)
- No startup health check — breakage is only discovered when a button is clicked.
- Every failure mode (missing binary / stale / crashed / disabled) shows the same unhelpful message.
- The install can leave a **stale** sidecar when a `--session-daemon` file-locks it and NSIS skips the
  replacement — and the version registry then lies (CPE-483). Nothing detects/repairs this.
- The `repos` sidecar was never bundled — a genuinely missing binary.
- No launch retry / crash auto-restart.

## Layers (decomposed just-in-time)
- **L1 — Diagnose + Repair** *(active, CPE-863)*: startup health sweep; per-sidecar status + real reason
  in Settings → Platform; a one-click **Repair**; specific launch-failure messages; bundle `repos`.
- **L2 — Auto-repair (true self-heal)** *(done, CPE-867)*: never-executed `.pristine` copy of each sidecar;
  startup sweep restores a missing/stale exe from it (after reaping a holding daemon) — zero user action.
- **L3 — Resilient launch** *(done, CPE-868)*: retry spawn/handshake with backoff. (Crash **auto-restart**
  while running is a further, optional piece — not yet needed; file a follow-up if crashes are observed.)

## Status
All three planned layers are shipped (L1 #138, L2 #145, L3 #146). Sidecars now diagnose + one-click Repair,
self-restore a missing/stale binary from a pristine copy, and retry a transient launch hiccup. **Epic
effectively complete** — only the optional crash-auto-restart remains, deferred until there's evidence it's
needed.

## Children
- CPE-863 — Sidecar health diagnosis + Repair button (L1) + bundle repos — **Done** (#138)
- CPE-867 — Auto-restore from a bundled pristine copy (L2) — **Done** (#145)
- CPE-868 — Resilient launch: retry with backoff (L3) — **Done** (#146)
