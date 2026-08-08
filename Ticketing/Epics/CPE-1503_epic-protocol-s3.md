---
id: CPE-1503
title: "EPIC: Network protocol — S3-compatible object-store provider (cpe-s3) [unlocks B2/GCS free]"
type: Task
status: Proposed
priority: Medium
component: Backend
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed:
---

> **Network Filesharing program (parent CPE-616). 2nd net-new protocol.** Filed 2026-08-08 (workshift PM,
> Network research). Dormant. **Overlaps CPE-616's own "cloud half" (its phase 4) — this IS that phase.**

## Why (high leverage — one provider, many backends)
S3 auth is *simpler* than interactive/OAuth cloud (static access-key + secret, SigV4 — no browser flow), so it
ranks easier than Drive/OneDrive. And **Backblaze B2, Google Cloud Storage, Wasabi, MinIO** are all
S3-compatible → they come **free** once S3 works (point the client at their endpoint).

## Scope
- New `crates/s3` (`cpe-s3`) implementing `FileSystemProvider` over an object store: bucket/prefix listing
  (paginated), get/put (multipart for large), stat, delete. Present keys as a virtual tree despite no real
  directories (needs CPE-1501's `has_real_dirs=false` capability).
- Auth: `AccessKey{id,secret}` (CPE-1501). Register `s3` scheme (already parsed in `location.rs`, currently
  "unsupported").
- **Crate decision at activation:** `rust-s3` (lighter, sync, MIT — but maintainer attention "oscillates",
  a flagged risk) vs `aws-sdk-s3` (official, safest, heavy tokio/hyper tree — tension with lean-core) vs
  **Apache `opendal`** (one crate, 50+ backends incl. s3/gcs/azblob — strategic if we want the long tail).
  Dependency Steward reviews the choice.

## Effort / deps / fit
Medium. Backend-only, headless-buildable (test against MinIO/localstack fixture). Deps: F1–F3 + F5 (capability
+ AccessKey auth). Flag lean-core tension on crate weight. Azure Blob = same bucket (add later if wanted).
