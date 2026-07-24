---
id: CPE-965
title: Concrete HS256 JWT TokenVerifier for the OIDC authenticator (cpe-security)
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-23
closed: 2026-07-23
epic: CPE-810
---

## Summary
The security layer (epic CPE-810) has the full 3-plane core + providers (CPE-816/817/818), incl. an
`OidcAuthenticator` that accepts an injectable `TokenVerifier` — but **no concrete verifier exists** (by
design the JWT/crypto machinery was kept out of the core crate). This adds a real **HS256 (HMAC-SHA256) JWT
verifier**, feature-gated (`jwt`, OFF by default) so the core stays dependency-light, so an OAuth/OIDC
bearer token can be verified end-to-end — not just with a test fake.

## Acceptance Criteria
- [x] `cpe-security` gains an OFF-by-default `jwt` feature (optional `hmac`/`sha2`/`base64` deps).
- [x] `jwt::HmacJwtVerifier` implements `TokenVerifier`: verifies the HS256 signature over `header.payload`,
      checks `exp`/`nbf` (with leeway) + optional `aud`, and maps `sub`/`iss`/`preferred_username` to
      `VerifiedClaims`. Rejects bad signature, wrong alg, expiry, malformed.
- [x] Tests (mint a token in-test): valid round-trip, tampered signature, expired, wrong alg, audience,
      and end-to-end through `OidcAuthenticator`.
- [x] CI (`crates` job) tests + clippies the `jwt` feature; default build stays lean (no crypto deps).

## Notes
Symmetric HS256 = the self-hosted-issuer common case; asymmetric RS256/JWKS (HTTP key fetch) is a further
provider. Clock is injectable for deterministic tests. Completes the OIDC path (CPE-817) with a working
verifier while honouring "JWT machinery is opt-in, out of the default core".
