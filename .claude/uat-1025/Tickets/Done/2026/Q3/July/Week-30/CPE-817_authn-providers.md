---
id: CPE-817
title: AuthN providers — API token, mTLS identity, OAuth/OIDC
type: feature
component: Backend
priority: medium
status: Done
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-810
estimate: 4h+
---

## Summary
Child of CPE-810. Full-stack authentication (decision: full security stack now): implement multiple
interchangeable `AuthN` providers behind the CPE-816 trait — **API token**, **mTLS client-cert
identity**, and **OAuth/OIDC** — and prove `any-passes` composition (accept a token *or* an OAuth
identity while both are registered). Establishes a `Principal` the AuthZ plane consumes. Prereq: CPE-816.

## Acceptance Criteria
- [x] API-token, mTLS-identity, and OAuth/OIDC providers each implement the `AuthN` trait.
- [x] `any-passes` composition verified: either credential authenticates with both registered.
- [x] Each yields a `Principal`; failures are audited and deny by default.
- [x] Provider parsing/verification unit-tested; clippy clean both modes.

## Work Log
2026-07-20 — Picked up. Estimate: 4h+ (unchanged). Prereq CPE-816 Done (merged #77) — the
`Authenticator` trait + registry + chain exist. Plan: add a dependency-free `authn` module to
`cpe-security` with three providers, each implementing `Authenticator`:
- **ApiTokenAuthenticator** — constant-time match of a presented token against a token→Principal map.
- **MtlsIdentityAuthenticator** — trusts the peer-cert subject the transport plane (CPE-818) put in
  `ctx.attributes`, maps it to a Principal (optional subject allowlist).
- **OidcAuthenticator** — verifies a bearer token via an injectable `TokenVerifier` trait (keeps
  JWKS/HTTP out of the core crate), maps verified claims → Principal, optional issuer allowlist.
Each **Abstains** when its credential isn't presented (so `any-passes` can fall through to the next),
Denies on an invalid credential. Prove `any-passes` end-to-end via ProviderRegistry+SecurityChain
(token OR oidc), and that no-credential → default-deny with an audit record. No new deps.
2026-07-20 — Built `crates/security/src/authn.rs` (module `cpe_security::authn`): `ApiTokenAuthenticator`
(constant-time `ct_eq` match against a token→Principal table, no early-out), `MtlsIdentityAuthenticator`
(maps the transport-verified `tls.client_cert.subject` attribute to a Principal; allowlist or accept-any),
and `OidcAuthenticator` (verifies a bearer via an injectable `TokenVerifier` trait → `VerifiedClaims` →
Principal; optional `iss` allowlist). All Abstain when their credential is absent, Deny on an invalid one.
2026-07-20 — Verified: `cargo test` 17/17 green (6 new authn tests incl. the end-to-end `any-passes`
proof that a token-only OR an oidc-only request authenticates while both are registered, and that a
no-credential request denies at the Authentication plane and is audited once); clippy `-D warnings`
clean. Caught + fixed a test-only bug (FakeVerifier used `split(':')` which broke on the issuer URL's
`://`; switched to `splitn(3, ':')`). No new deps; `src-tauri` untouched (both app modes unaffected).
No CI change — the `Server crates` job already lints+tests `crates/security` on all 3 OSes.

## Resolution
Added the reference **authentication providers** as a dependency-free `authn` module in `cpe-security`,
each implementing the CPE-816 `Authenticator` trait:
- **`ApiTokenAuthenticator`** — constant-time (`ct_eq`) match of a presented token (context attribute
  `auth.api_token`) against a `token → Principal` table.
- **`MtlsIdentityAuthenticator`** — trusts the client-cert subject the transport plane (CPE-818) verified
  and placed in `tls.client_cert.subject`, mapping it to a Principal (subject allowlist, or accept-any).
  It does not do the TLS handshake — that is the transport plane's job.
- **`OidcAuthenticator`** — verifies an OAuth/OIDC bearer (`auth.bearer`) via an **injectable
  `TokenVerifier`** so JWKS/JWT/HTTP stays out of the core crate; maps `VerifiedClaims` → Principal;
  optional issuer allowlist. A real verifier is wired at integration (CPE-820).

**Composition contract:** each provider `Abstain`s when *its* credential isn't presented, so an
`AnyPasses` AuthN plane falls through provider-to-provider; it only `Deny`s when a presented credential
fails to verify. No credential at all → the core's structural default-deny (audited).

Files: `crates/security/src/authn.rs` (+ `pub mod authn;` in `lib.rs`). 6 unit tests added (17 total).

Scope/tradeoffs:
- OIDC verification is abstracted behind `TokenVerifier` rather than pulling a JWT/JWKS/HTTP stack into
  the pure core crate — keeps it headless-testable and dependency-light; the concrete verifier lands with
  the remote loop (CPE-820).
- mTLS *identity* (AuthN) is separated from the mTLS *handshake* (transport security, CPE-818): this
  provider consumes an already-verified subject attribute, keeping the planes cleanly decoupled.