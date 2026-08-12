---
id: CPE-1681
title: "cpe-s3 crate foundation — S3Config, path-style vs virtual-host addressing, and the SigV4 signer"
type: Feature
status: Backlog
priority: Medium
component: Backend
tags: [ready]
epic: CPE-1503
estimate: M
created: 2026-08-12
closed:
---

## What

Create `crates/s3` (`cpe-s3`), the sibling of `cpe-sftp` / `cpe-webdav` / `cpe-ftp`, and land the two pieces
every later ticket in this epic sits on: **where a request is addressed** and **how it is signed**. No
`FileSystemProvider` impl yet — that is CPE-1683 and CPE-1684. This ticket is the foundation and it is what
unblocks them, so build it first.

## Why the crate choice is already made — do not re-open it

The epic brief left the crate open (`rust-s3` vs `aws-sdk-s3` vs `opendal`) and the activation closed it:
**hand-roll the five requests over `ureq` + `roxmltree`, exactly as `cpe-webdav` does.** `ureq` (rustls),
`roxmltree`, `hmac` and `sha2` are all already in the tree, so this adds **no new dependency family**;
every alternative drags either an async runtime or a second HTTP/TLS stack into a codebase that has
deliberately kept every remote provider sync and `ring`-backed. Read `crates/webdav/Cargo.toml` — its
comments spell out why each dep is there — and mirror it.

## Scope

- **`S3Config`**: endpoint, region, bucket, addressing style, and credentials (access key id + secret).
- **Addressing.** This is the whole "unlocks B2/GCS/Wasabi/MinIO free" claim in the epic title, and it is
  not free — it is this field. The same `(bucket, key)` must produce
  `https://bucket.s3.us-east-1.amazonaws.com/key` under virtual-host addressing and
  `http://localhost:9000/bucket/key` under path-style, because MinIO and most self-hosted gateways only do
  path-style while AWS deprecated it for new buckets. Get the URL construction wrong and every request
  404s against half the ecosystem, so this is where the tests belong.
- **SigV4 signer.** Canonical request → string-to-sign → signing key → `Authorization` header, over the
  `hmac`/`sha2` already in `cpe-server`'s dep set. Include the payload hash
  (`x-amz-content-sha256`) and `x-amz-date` handling; sign the headers that actually matter rather than
  everything, and be explicit about the canonical URI/query encoding rules — that encoding is where
  hand-rolled SigV4 implementations traditionally go wrong.

## Verify (headless)

AWS publishes a **SigV4 test suite** with fixed inputs, a fixed date, and the exact expected canonical
request, string-to-sign, and `Authorization` header. Use those vectors — they are the reason this ticket
can be verified with no network, no credentials and no bucket. Do not settle for a self-consistent
round-trip test that would pass on a wrong-but-stable implementation.

## Acceptance criteria

- [ ] `crates/s3` builds standalone (out of any workspace, like its three siblings) and adds no dependency
      family not already present in the repo — name each dep and where it is already used.
- [ ] The signer reproduces the published AWS SigV4 vectors byte for byte, at each of the three
      intermediate stages, not just the final header.
- [ ] The same `(bucket, key)` yields the correct URL under both addressing styles, covered by a test for
      each, including a key needing percent-encoding.
- [ ] Changing one character of the signing key derivation, or flipping the addressing style, turns a test
      red — the tests fail for the right reason.
- [ ] `cargo test` green; `cargo clippy --all-targets -D warnings` clean.

## Notes

Filed by the sprint PM at the CPE-1503 activation, 2026-08-12. The secret access key must never be logged
or included in an error message — it is the one value in `S3Config` that is genuinely secret, and it will
arrive from the OS keychain via CPE-1685. Check that no `Debug` derive leaks it.

Prereq for CPE-1682, CPE-1683, CPE-1684. Model the crate shape on `crates/webdav/src/lib.rs`.
