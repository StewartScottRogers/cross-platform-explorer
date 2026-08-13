---
id: CPE-1689
title: cpe-s3 collapses four distinct object keys onto one URL, and lets anything at all into the signed Host header
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Four findings from the independent UAT on PR #867. None blocked that PR — the signer and the addressing
logic are correct, and the UAT confirmed them against an independently-written reference implementation —
but all four are in code that CPE-1684 (object ops) is about to build on, so they are cheaper to fix now
than after there is a caller.

Worth noting how these were found: the UAT wrote its **own** SigV4 implementation from the specification
and diffed 180 URL/host/path/Authorization rows against the crate, then reproduced three published AWS
vectors that appear nowhere in PR #867's own tests. That is the standard these findings come from.

### D1 — Leading slashes are stripped, so four distinct keys become one object *(the important one)*

```
key "a.txt"     -> https://my-bucket.s3.us-east-1.amazonaws.com/a.txt
key "/a.txt"    -> https://my-bucket.s3.us-east-1.amazonaws.com/a.txt
key "//a.txt"   -> https://my-bucket.s3.us-east-1.amazonaws.com/a.txt
key "///a.txt"  -> https://my-bucket.s3.us-east-1.amazonaws.com/a.txt
```

`object_target` uses `key.trim_start_matches('/')`, which removes **every** leading slash. S3 keys
beginning with `/` are legal, and they turn up routinely from naive path joins — so a user with such a key
silently reads or writes **the wrong object** instead of getting an error. Silent wrong-object is the worst
available outcome for a file tool.

It also contradicts the crate's own stated principle: the module doc says keys are opaque byte strings and
that `a//b` must survive unchanged. It does survive — in the middle. Not at the front.

### D2 — The endpoint is not validated at all, while the bucket is

`validate_bucket` rejects `/ ? # \` and whitespace. Nothing equivalent guards the endpoint, so control
characters flow straight into the signed `Host` header:

```
endpoint "https://s3.example.com\r\nX-Injected: 1"  ->  host = "s3.example.com\r\nX-Injected: 1"

canonical request:
  host:s3.example.com
  X-Injected: 1
```

That is the classic request-splitting shape. It is **not exploitable today** — this crate issues no HTTP
requests yet — which is exactly why it should be closed before CPE-1684 gives it a transport.

Two more from the same missing validation:

- `https://user:pw@s3.amazonaws.com` yields `host = "user:pw@s3.amazonaws.com"`, putting a **password into
  the signed canonical request and into `RequestTarget`'s `Debug`** — the one place this crate otherwise
  works hard to keep credentials out of.
- `endpoint_parts()` takes the host as everything before the **last** colon, so that same URL parses its
  host as `"user"` and silently drops to path-style addressing.

The endpoint will arrive from a typed connection profile (CPE-1685/1686), so "the user would have to type
something malformed" is a reason it is low severity, not a reason to leave it.

### D3 — `sign()` does not enforce the `host` header it documents as mandatory

The doc says *"`headers` must include `host` — S3 rejects a signature that does not cover it."* Signing with
an empty header list succeeds and produces `SignedHeaders=""`. That yields precisely the opaque server-side
failure the module says it exists to prevent.

### D4 — `method` is neither uppercased nor validated

Documented as "Uppercase HTTP method". `"get"`, `"Get"` and `" GET "` each sign to a different, silently
wrong signature.

### D5 — AWS China is treated as not-AWS *(added from the PR #867 review)*

`AddressingStyle::Auto` decides "is this AWS?" by matching the `.amazonaws.com` registrable suffix.
**`s3.cn-north-1.amazonaws.com.cn` is genuine AWS and fails that test**, so every China-region bucket
silently drops to path-style. The rule's own stated question answers wrong. Reviewer's probe:

```
https://s3.us-east-1.amazonaws.com           -> VirtualHost
https://s3.cn-north-1.amazonaws.com.cn       -> Path          <-- gap
https://amazonaws.com.attacker.example       -> Path          (correct — not spoofable)
```

Path-style still *works* against AWS, so this is a correctness-of-intent bug rather than a breakage — but
the code claims to answer a question it gets wrong for a whole partition. One-line fix.

### D6 — `validate_bucket` allows `@` and `:` *(defence in depth, not live)*

A bucket named `evil@attacker.example` under an **explicit** `with_addressing(VirtualHost)` produces the
host `evil@attacker.example.s3.amazonaws.com`. Unreachable through `Auto` — such buckets resolve to
path-style, where the bucket is percent-encoded — so this is not a live bug. It is the same missing-input-
validation class as D2 and should be closed with it.

### D7 — `Signer` is `pub`, derives `Debug`, and the redaction guard does not cover it

Safe today: it holds a `Credentials`, whose redaction is guarded. But nothing goes red if a future refactor
stores an owned secret on `Signer` directly. The guard should cover the type, not just the type it
currently delegates to.

### D8 — The "one encoder, cannot drift" guarantee covers the path only

`RequestTarget.url` carries no query string, so the encoded path used in the URL and in the signature come
from one construction — which is the property that stops the classic silent SigV4 failure. **There is no
equivalent for the query.** CPE-1683 has to rebuild the on-wire query itself and can drift from
`canonical_query`, reintroducing exactly the bug this design avoided for the path. Export a query builder
before the request layer lands.

## Scope

`crates/s3` — `object_target`, `endpoint_parts`/endpoint handling, and `sign`.

- D1: preserve leading slashes, or reject them explicitly. Do **not** silently normalise — either the key
  is opaque (the crate's stated principle) or it is validated, but not quietly rewritten.
- D2: validate the endpoint at least as strictly as the bucket. Reject control characters and whitespace
  outright; decide deliberately what to do with userinfo (rejecting is fine and probably right) and fix the
  last-colon host split while you are there.
- D3: make the documented requirement real — refuse to sign without a `host` header.
- D4: uppercase or reject; either is fine, silently signing the wrong thing is not.

## Acceptance criteria

- [ ] `/a.txt`, `//a.txt` and `a.txt` are three different objects, or the first two are refused with a
      message saying why. They must not collide.
- [ ] An endpoint containing CR, LF, or other control characters is refused, and a test asserts nothing
      reaches the canonical request.
- [ ] An endpoint with userinfo is refused (or its credentials provably never reach `Debug` or the signed
      request) — and the host split no longer breaks on the embedded colon.
- [ ] Signing without a `host` header fails with a message naming the missing header.
- [ ] A lowercase or space-padded method either signs identically to the uppercase form or is refused.
- [ ] An AWS China endpoint (`s3.cn-north-1.amazonaws.com.cn`) is recognised as AWS and gets virtual-host
      addressing for a DNS-safe bucket — with `amazonaws.com.attacker.example` still correctly refused.
- [ ] `validate_bucket` rejects `@` and `:`.
- [ ] The secret-redaction guard covers `Signer` itself, so storing an owned secret on it turns a test red.
- [ ] A query-string builder is exported so the on-wire query and `canonical_query` come from **one**
      construction, the way the path already does — CPE-1683 must not have to rebuild it.
- [ ] Each guard broken **on its own** turns a **distinct** test red, with the real output pasted in the PR,
      per the Evidence Rules in `Ticketing/wiki.md`.

## Also worth fixing while in the file (from the same UAT, non-defect)

- `s3_list_objects_vector_matches` asserts the canonical query with `starts_with` rather than equality — it
  would tolerate trailing garbage. Airtight only because the pinned signature below it carries the load.
- `debug_output_never_contains_the_secret` exercises only `{:?}`. `{:#?}` was verified clean by hand but is
  not covered.
- `flipping_the_addressing_style_changes_the_signature` is an `assert_ne!`, which would pass on almost any
  wrong implementation. Supplementary only.
- The module doc claims SigV4 exempts quoted strings from whitespace collapse and that this crate
  deliberately deviates. The published `get-header-value-trim` vector collapses **inside** the quoted string
  too, so the behaviour is right and the doc describes a deviation that does not exist. Delete the claim.

## Notes

Filed by the Foreman from the PR #867 UAT, 2026-08-12. **Do before CPE-1684** (object ops), which is the
first slice that will actually issue requests with these URLs.

Related: **CPE-1681** (the foundation this reviews), **CPE-1684** (the caller), and the recurring rule —
*silently doing something different is worse than failing loudly*.
