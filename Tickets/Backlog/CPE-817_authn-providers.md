---
id: CPE-817
title: AuthN providers — API token, mTLS identity, OAuth/OIDC
type: feature
component: Backend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-20
epic: CPE-810
estimate: 4h+
---

## Summary
Child of CPE-810. Full-stack authentication (decision: full security stack now): implement multiple
interchangeable `AuthN` providers behind the CPE-816 trait — **API token**, **mTLS client-cert
identity**, and **OAuth/OIDC** — and prove `any-passes` composition (accept a token *or* an OAuth
identity while both are registered). Establishes a `Principal` the AuthZ plane consumes. Prereq: CPE-816.

## Acceptance Criteria
- [ ] API-token, mTLS-identity, and OAuth/OIDC providers each implement the `AuthN` trait.
- [ ] `any-passes` composition verified: either credential authenticates with both registered.
- [ ] Each yields a `Principal`; failures are audited and deny by default.
- [ ] Provider parsing/verification unit-tested; clippy clean both modes.

## Work Log
