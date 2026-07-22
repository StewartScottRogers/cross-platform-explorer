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
- **L2 — Auto-repair (true self-heal)** *(planned)*: bundle a never-executed `.pristine` copy of each
  sidecar; on startup/launch-failure auto-restore any missing/stale/wrong-version binary from it (reaping
  a holding daemon first) — zero user action.
- **L3 — Resilient launch** *(planned)*: retry spawn/handshake with backoff; auto-restart a crashed
  sidecar; re-check health after.

## Children
- CPE-863 — Sidecar health diagnosis + Repair button (L1) + bundle repos — **Done** (shipped via #138)
- (L2) Auto-repair from a bundled pristine copy — not yet ticketed
- (L3) Resilient launch: retry + auto-restart — not yet ticketed
