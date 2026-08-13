---
id: CPE-1681
title: "cpe-s3 crate foundation — S3Config, path-style vs virtual-host addressing, and the SigV4 signer"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
epic: CPE-1503
estimate: M
created: 2026-08-12
closed: 2026-08-12
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

- [x] `crates/s3` builds standalone (out of any workspace, like its three siblings) and adds no dependency
      family not already present in the repo — name each dep and where it is already used.
- [x] The signer reproduces the published AWS SigV4 vectors byte for byte, at each of the three
      intermediate stages, not just the final header.
- [x] The same `(bucket, key)` yields the correct URL under both addressing styles, covered by a test for
      each, including a key needing percent-encoding.
- [x] Changing one character of the signing key derivation, or flipping the addressing style, turns a test
      red — the tests fail for the right reason.
- [x] `cargo test` green; `cargo clippy --all-targets -D warnings` clean.

## Notes

Filed by the sprint PM at the CPE-1503 activation, 2026-08-12. The secret access key must never be logged
or included in an error message — it is the one value in `S3Config` that is genuinely secret, and it will
arrive from the OS keychain via CPE-1685. Check that no `Debug` derive leaks it.

Prereq for CPE-1682, CPE-1683, CPE-1684. Model the crate shape on `crates/webdav/src/lib.rs`.

## Work Log

2026-08-12 — Built `crates/s3` (`cpe-s3`): `lib.rs` (`Credentials`, `S3Config`, `AddressingStyle`,
`RequestTarget`) + `sigv4.rs` (the signer). 23 tests, all offline.

**Dependencies — two, both already in the tree:** `hmac` 0.12 (same version `crates/security` uses for
its `jwt` feature) and `sha2` 0.10 (a direct dep of `crates/server`). No HTTP client and no XML parser
yet: this slice is pure computation, so `ureq`/`roxmltree`/`cpe-server` arrive with the request layer in
CPE-1683/1684 rather than sitting here unused.

**Vectors verified** (each reproduced exactly; every one is published by AWS, none hand-computed):

| Vector | Stage(s) asserted | Expected value |
|---|---|---|
| SigV4 docs, "derive a signing key" (`20150830`/`us-east-1`/`iam`) | signing key | `c4afb1cc…a4b9` |
| aws-sig-v4-test-suite `get-vanilla` | canonical request + string-to-sign + `Authorization` | `5fa00fa3…bf31` |
| aws-sig-v4-test-suite `get-vanilla-query-order-key-case` | canonical query order + `Authorization` | `b97d918c…2500` |
| S3 docs, GET Object (`/test.txt`, `Range`) | canonical request + string-to-sign + signature | `f0e8bdb8…db41` |
| S3 docs, PUT Object (body + `x-amz-storage-class`) | payload hash + signed headers + signature | `98ad7217…08bd` |
| S3 docs, GET Bucket / List Objects (`max-keys=2&prefix=J`) | canonical query + signature | `34b48302…dc6f7` |

One recalled value was **wrong** and the test caught it: the `get-vanilla` string-to-sign digest was
first written as `816cd5b4…`, went red, and the computed `bb579772…` was adopted only after the vector's
own `.creq` and `.authz` both matched — a wrong string-to-sign cannot produce the published signature, so
that line is pinned by the two vectors around it. Noted in the test's doc comment.

**Addressing.** `AddressingStyle::{Auto, VirtualHost, Path}`, `Auto` by default. `Auto` resolves by two
questions: (1) can the bucket be a DNS label at all — a dotted/uppercase/underscored/short name gets
path-style, because `my.bucket.s3.amazonaws.com` fails the `*.s3.amazonaws.com` wildcard certificate
before S3 ever sees it; (2) is the endpoint host under `amazonaws.com` (matched on the registrable
suffix, so `amazonaws.com.attacker.example` is not AWS) — if so virtual-host, else path-style.
Path-style is the compatible answer, so `Auto` only leaves it where virtual-host is actually required.

**One encoder, one output.** `RequestTarget` returns `url`, `host` and `encoded_path` from a single
construction, and the signer takes the already-encoded path — so the URL on the wire and the canonical
URI cannot drift into a `SignatureDoesNotMatch` with nothing in the message to explain it. S3's canonical
URI is encoded **once** and never path-normalized (`a//b` and `a/../b` are distinct keys); both are
tested.

**Secret handling.** `Credentials` keeps the secret private behind `secret()`, with a hand-written
`Debug` that renders `<redacted>`, so `S3Config`'s derived `Debug` is safe by construction. Guard test
`debug_output_never_contains_the_secret` covers both. No error message in the crate formats the secret.

**Guard neutralisation** (each break reverted, suite re-confirmed green): `AWS4`→`AWS5` in the signing-key
derivation reddened 7 tests including `signing_key_matches_the_published_derivation_example`; flipping the
`Auto` addressing branch reddened 7 addressing tests; making the query encoder keep `/` reddened only
`path_and_query_encoders_differ_only_in_how_they_treat_slash`; un-redacting the secret reddened only
`debug_output_never_contains_the_secret`; loosening the `x-amz-date` check reddened only
`malformed_amz_date_is_rejected_without_leaking_the_secret`. Real output in the PR body.

Also wired `crates/s3` into `.github/workflows/ci.yml` (rust-cache workspace list + its own clippy/test
step on the 3-OS matrix) — a standalone crate that is not listed there is never compiled by CI at all —
and added `crates/*/target/` to `.gitignore`, which was missing for every standalone crate.

Left to the later tickets on purpose: error mapping (CPE-1682), `ListObjectsV2` (CPE-1683), object ops
(CPE-1684), `cpe_vfs::open` routing (CPE-1685), frontend + docs (CPE-1686).
