---
id: CPE-1695
title: cpe-s3 trims header values more widely than SigV4 does, and lets the other C0 controls through
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Two header-value hygiene issues in `crates/s3/src/sigv4.rs`, both deliberately left out of CPE-1691's
scope by the worker and reviewer who found them, both worth doing together since they live in the same
function.

### 1. `normalize_header_value` trims Unicode whitespace; SigV4 trims SP and HTAB only

`sigv4.rs:160` uses `str::trim()`, which strips **all** Unicode whitespace. The SigV4 spec trims only
space (0x20) and horizontal tab (0x09), then collapses sequential spaces.

So a header value with a leading NBSP (U+00A0) — or any other Unicode space — is trimmed on our side and
**not** trimmed by S3. The two canonical requests diverge, the signatures do not match, and the client
gets an opaque `SignatureDoesNotMatch` with nothing pointing at the whitespace.

This is a **correctness / interop** bug, not a security one: it makes a legitimate request fail, it does
not let an illegitimate one through. It is pre-existing — it predates CPE-1689 and CPE-1691.

### 2. VT, FF and the other C0 controls survive in a header value

CPE-1691 settled on a deliberately narrow header-value rule — CR, LF and NUL only — on the grounds that
those are the characters that let a value **escape its own line**, and that S3 legitimately carries
near-arbitrary bytes in some header values. That reasoning is sound and should not be casually widened.

But RFC 7230 forbids the whole C0 range in `field-content`, and `\x0b` (VT), `\x0c` (FF) and friends
currently pass. They cannot restructure the canonical request, so this is an interop hazard rather than
an injection risk.

**Do not simply widen the rule to all of 0x00–0x1F.** The CPE-1691 worker's explicit recommendation, on
declining to fix it under time pressure, was that this deserves its own investigation into *which*
controls actually break *which* client — not a scope expansion decided in passing. Honour that: find out
before tightening, and write down what you found.

## Scope

`crates/s3/src/sigv4.rs` — `normalize_header_value` and `reject_framing_bytes`.

## Acceptance criteria

- [ ] `normalize_header_value` trims SP and HTAB only, matching the SigV4 spec, and a test pins a value
      with a leading NBSP to prove the character survives normalisation rather than being stripped.
- [ ] The sequential-space collapsing behaviour is unchanged and still passes the existing
      `get-header-value-trim` vector.
- [ ] A decision on the remaining C0 controls, **with the investigation behind it written down** — which
      controls, which clients, what actually breaks. Either tighten with that evidence, or record why
      not. An undocumented "we widened it to be safe" does not satisfy this.
- [ ] Every published AWS test vector the crate checks still produces its exact expected signature — this
      change touches the canonicalisation path, so an unchanged known-answer result is the proof that
      valid requests still sign identically.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per
      the Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #872 review round, 2026-08-12. Item 1 was found by the independent
reviewer (its finding L3), item 2 by the same reviewer (L1) with the worker's declining rationale
recorded above. Neither blocked CPE-1691; both were correctly judged out of its scope.

Related: **CPE-1691** (the validation standard these sit beside), **CPE-1689** (which started it).
