---
id: CPE-1503
title: "EPIC: Network protocol — S3-compatible object-store provider (cpe-s3) [unlocks B2/Wasabi/MinIO; GCS TBD]"
type: Task
status: In Progress
priority: Medium
component: Backend
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed:
---

> **Network Filesharing program (parent CPE-616). 2nd net-new protocol.** Filed 2026-08-08 (sprint PM,
> Network research). Dormant. **Overlaps CPE-616's own "cloud half" (its phase 4) — this IS that phase.**

## Why (high leverage — one provider, many backends)
S3 auth is *simpler* than interactive/OAuth cloud (static access-key + secret, SigV4 — no browser flow), so it
ranks easier than Drive/OneDrive. **Backblaze B2, Wasabi and MinIO** are S3-compatible → they come **free**
once S3 works (point the client at their endpoint).

**Claim narrowed 2026-08-12 — Google Cloud Storage is no longer part of the "free" list.** The CPE-1681
worker, having just built the SigV4 signer, flagged that GCS's S3 shim (the XML API) carries its own
SigV4 quirks and **does not support ListObjectsV2 the same way** — which is precisely the call CPE-1683 is
built on. Compatible addressing is necessary but not sufficient. GCS is therefore **explicitly undecided**
and must be scoped in or out at CPE-1683, on evidence rather than on this line. B2, Wasabi and MinIO alone
still carry the epic's rationale. Recorded per the Evidence Rules in `Ticketing/wiki.md`: state the scope
of a claim rather than letting a headline outrun what was verified.

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

## ACTIVATED 2026-08-12 (sprint PM) — every dependency this epic named is already in the tree

Chosen over the other dormant epics because it is the only one that is **fully headless-buildable with
nothing left to decide by hand**: no hardware, no browser consent, no model key, no GUI taste call. The
foundation it was waiting on has since landed, and the seams were left in the code *by name*:

- `cpe_server::connections::AuthMethod::AccessKey { id, secret_ref }` exists, with a doc comment that says
  in so many words "unblocks CPE-1503 (S3)" (CPE-1515).
- `cpe_server::provider::ProviderCapabilities::has_real_dirs` exists, and `provider.rs` carries a test
  literally named `a_provider_can_override_capabilities_eg_s3_style_no_real_dirs`.
- `Scheme::S3` already parses in `location.rs` and `fs_route.rs` already routes it to a "not connected"
  message. **Note (CPE-1686, 2026-08-12): the `s3://bucket/key` → host=`bucket` reading in this line was
  wrong and is superseded.** It leaves no field for the endpoint *or* the region, which makes a custom
  endpoint inexpressible and would have broken this epic's own "B2/GCS/Wasabi/MinIO come free" claim.
  The settled convention is `host` = endpoint, `port` = endpoint port (blank ⇒ 443), `user` = region
  (blank ⇒ `us-east-1`), `path` = `/bucket[/prefix]`. `location.rs`'s parser is scheme-agnostic and
  handles it with no new arm — verified against the real parser, not assumed.
- `cpe_vfs::open` has the hole to fill: `s3` currently falls through to `unsupported scheme 's3'`, and all
  three shipped providers return *"reserved for a future S3/cloud provider"* for `AccessKey` auth.
- `cpe-ftp` (CPE-1514, the sibling protocol epic) proved the whole recipe end to end three months' worth of
  tickets ago: a standalone provider crate + a scheme arm in `cpe_vfs::open`.

Grep confirms there is **no** S3 or SigV4 code anywhere in the repo — the only two hits for "SigV4" are the
`AccessKey` doc comment and its generated copy in `bindings.gen.ts`. This is genuinely unbuilt.

### Decisions taken at activation (user away — logged, not asked)

- **Crate choice: none of the three the brief listed. Hand-roll the five requests over `ureq` + `roxmltree`,
  exactly as `cpe-webdav` does.** The brief left this open for the Dependency Steward; the PURPOSE.md
  tiebreaker settles it. `aws-sdk-s3` drags a tokio/hyper/aws-lc tree into a codebase whose every remote
  provider is deliberately sync with a `ring`-backed rustls; `opendal` is the same weight problem wearing a
  strategic hat; `rust-s3` carries the maintainer risk the brief itself flagged *and* its own HTTP stack.
  The S3 REST surface we actually need is five requests (ListObjectsV2, HEAD, GET, PUT, DELETE) and one
  signing algorithm. `ureq`+rustls, `roxmltree`, `hmac` and `sha2` are **all already in the tree** — this
  adds no new dependency family at all. If a sixth or seventh S3 API ever becomes necessary, revisit.
- **Virtual directories via the ListObjectsV2 `delimiter`**, not a client-side key split: it is what every
  S3-compatible server implements, it paginates correctly, and it keeps the listing cost proportional to
  one level rather than to the whole bucket.
- **`rename` is refused honestly**, not emulated with copy-then-delete. S3 has no atomic rename;
  `supports_rename = false` plus a clear error beats a silent non-atomic move that can lose data halfway.
- **v1 is credential-only auth** (`AccessKey`) — no STS, no instance metadata, no assumed roles.
- **Multipart upload is out of v1.** `FileSystemProvider::write` already takes a whole `&[u8]`, so the
  5 GB single-PUT ceiling is not the binding constraint; note it, don't build it.

### Child tickets (filed 2026-08-12)

1. **CPE-1681** — `cpe-s3` crate foundation: `S3Config` + path-style/virtual-host endpoint addressing +
   the SigV4 signer, verified against AWS's published test vectors. *Ready — build first, unblocks the rest.*
2. **CPE-1682** — S3 error responses name the real cause (`AccessDenied`/`NoSuchBucket`/
   `SignatureDoesNotMatch`), bounded and depth-guarded. *(prereq: 1681)*
3. **CPE-1683** — `S3Provider::list`: ListObjectsV2 with delimiter + continuation pagination →
   virtual directories, `has_real_dirs = false`. *(prereq: 1681, 1682)*
4. **CPE-1684** — `S3Provider` object ops: stat/read/write/delete/mkdir-marker + the honest `rename`
   refusal. *(prereq: 1681, 1682)*
5. **CPE-1685** — Route `s3` through `cpe_vfs::open`: `AccessKey` → credentials, the missing-secret guard,
   `default_port`. *(prereq: 1683, 1684)*
6. **CPE-1686** — Frontend: `s3` as a savable scheme + an access-key auth kind in the connection form +
   the `31-network.md` docs page. *Ready — independent of the whole backend chain.*

## Work Log

2026-08-12 (sprint PM) — **Activated.** Verified the foundation is live (AccessKey auth, `has_real_dirs`,
`Scheme::S3`, `fs_route`, the `cpe_vfs::open` hole, the `cpe-ftp` precedent) and that no S3/SigV4 code exists
yet. Resolved the brief's one open question (the crate choice) against the PURPOSE.md tiebreaker — hand-roll
over the deps already present, adding no new dependency family. Decomposed into CPE-1681–1686; CPE-1681 and
CPE-1686 are pickable immediately, the other four fall out behind CPE-1681.

2026-08-13 (CPE-1683 worker) — **GCS claim corrected, not merely narrowed.** The 2026-08-12 note above says
GCS's XML API "does not support ListObjectsV2 the same way", flagged by the CPE-1681 worker as unverified.
Checked live against GCS's current published XML API reference for this ticket
(`docs.cloud.google.com/storage/docs/xml-api/get-bucket-list` and `.../storage/docs/interoperability`,
fetched 2026-08-13, not recalled from training data): the documented request/response shape is a
**superset** of what `CPE-1683` sends and parses — `list-type=2`, `delimiter`, `continuation-token`,
`start-after` are all explicitly documented parameters, and the response documents
`IsTruncated`/`NextContinuationToken`/`CommonPrefixes`, with no caveat text on either page about a
ListObjectsV2 incompatibility. **The specific claim in the previous note is wrong**, not merely unverified.
What remains genuinely unverified is SigV4 signing parity end to end (no GCS account/credentials/network
egress in this headless environment to test a live signed request) — GCS's own docs describe "a V4 signing
process" and HMAC credentials without confirming byte-for-byte canonicalisation parity with AWS SigV4.
**Decision for v1: GCS is treated like any other undedicated S3-compatible gateway — expected to work by
protocol shape, not verified end to end, no GCS-specific code anywhere in `crates/s3`.** A live-conformance
ticket against a real GCS bucket (mirroring the QNAP-NAS precedent already used for SFTP/WebDAV/FTP) is the
natural follow-up once credentials are available; filing it is a resourcing call, not a scoping one, so it
is not filed here. See `crates/s3/src/provider.rs`'s top doc comment for the full reasoning.
