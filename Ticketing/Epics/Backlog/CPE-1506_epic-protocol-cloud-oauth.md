---
id: CPE-1506
title: "EPIC: Network — cloud OAuth providers (Google Drive / OneDrive / Dropbox) — SEPARATE track"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed:
---

> **Network Filesharing program (parent CPE-616). A SEPARATE track — do NOT interleave with the protocol
> difficulty ladder.** Filed 2026-08-08 (sprint PM, Network research). Dormant. Consistent with CPE-616's
> own decision to defer OAuth cloud providers.

## Why it's a different problem shape
Drive/OneDrive/Dropbox aren't "mount a share" — they need an **OAuth2 browser-consent flow, token
storage/refresh, and revocation handling**. That's a fundamentally different UX/engineering problem than a
protocol client, which is exactly why it's its own track (after S3, which covers the key-auth object stores
including B2/GCS for free).

## Scope (per provider, layered on the same FileSystemProvider trait via CPE-1501's Token/OAuth auth)
- **Google Drive** — `google-drive3` (auto-generated from the official API, includes OAuth2). Most viable.
- **OneDrive** — `onedrive-api` (its own docs warn some Graph APIs are beta / **not production-recommended** —
  maturity risk).
- **Dropbox** — **no strong Rust SDK found** → likely hand-rolled REST. Highest effort; consider deferring.
- Shared: OAuth consent (attended browser flow — NOT headless), token refresh/revoke, secret storage via
  CPE-1497 keychain.

## Effort / deps / fit
Large (mostly OAuth-flow/UX, not protocol). **Not headless** (browser consent is attended → a QA-burndown /
user-resource item). Deps: F1–F3 + F5 (Token auth) + CPE-1503 (S3) shipped first. Sequence LAST of the network
work. `opendal` could alternatively cover several cloud backends behind one dep — evaluate.

## Also captured (no separate epics — see research-library)
**AFP: DO NOT BUILD** (Apple removes it in macOS 27; no Rust crate; zero forward value → tell users to switch
their NAS to SMB3). **rclone-shell-out**: a fallback *strategy* for the niche long tail, not the default.
**iSCSI / Git-over-SSH / MEGA**: out of scope (block device / VCS / proprietary crypto).
