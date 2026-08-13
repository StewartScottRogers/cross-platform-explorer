---
id: CPE-1691
title: cpe-s3 validates the endpoint and the bucket, then signs whatever you put in a header, a region, or a key id
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-12
closed: 2026-08-12
---

## Problem

Found by the independent UAT on PR #868 — the PR that *added* endpoint validation. It closed the
request-splitting shape at one door and left three others open, which is worth fixing as one job rather
than discovering one at a time.

**Do this before CPE-1684**, for exactly the reason CPE-1689 was done before it: today `crates/s3` issues
no HTTP requests, so none of this is live. CPE-1684 is the slice that gives it a transport.

### 1. `sign()` does not validate header values at all *(the one that matters)*

```rust
sign(headers: [("host", "h.example"),
               ("x-amz-copy-source", "/bucket/dir/evil\r\nX-Amz-Acl: public-read")])
```

produces a **nine-line canonical request** with `X-Amz-Acl: public-read` standing on its own line. This is
the identical shape CPE-1689 refused at the endpoint, at the general entrance instead.

What makes it more than theoretical: **S3 object keys legally contain CR and LF**, and CPE-1684 will build
`x-amz-copy-source` from a key. So the untrusted data and the vulnerable sink are about to be wired
together by the next ticket in this epic.

### 2. `region` and `access_key_id` are unvalidated and land raw in `Authorization`

```
region "us-east-1\r\nX-Injected: 1"
  -> Credential=AKIA…/20130524/us-east-1\r\nX-Injected: 1/s3/aws4_request
```

Both come straight from a saved connection profile once CPE-1685 lands.

Worse, the check that *does* exist is enforced on only one of two public paths: `target_for` rejects an
empty region, but `Signer::new(&creds, "")` signs happily and produces `scope="20130524//s3/aws4_request"`.
A validation that one entry point applies and another skips is barely a validation.

### 3. `validate_bucket` is weaker than `validate_endpoint_text` — D6 is half-applied

`\0`, `\x0b`, `\x7f` and non-ASCII all pass the bucket check (`\r` and `\n` happen to be caught by
`is_ascii_whitespace`). Under an **explicit** `with_addressing(VirtualHost)` that yields
`host = "a\0b.s3.amazonaws.com"` in the signed request.

The UAT confirmed `Auto` is safe — such buckets resolve to path-style, where the bucket is percent-encoded
as `a%00b` — so this is hygiene rather than a live hole. But CPE-1689 set the standard for endpoint text
and the bucket should meet it; two validators of different strictness on the same kind of input is how the
weaker one gets forgotten.

### 4. D3 checks that `host` is present, not that it is usable

`("host", "")` and `("host", "   ")` are accepted and sign `host:` with `SignedHeaders=host`. Two different
`host` values merge to `host:a.example,b.example` — spec-correct merging, but both outcomes are precisely
the "signs cleanly, then fails opaquely at the server" result D3 exists to prevent.

## Scope

`crates/s3` — `sign`/`SigningInput`, `Signer::new`, and `validate_bucket`.

Apply **one** validation standard to every piece of caller-supplied text that reaches the canonical request
or the `Authorization` header. The natural shape is to reuse whatever `validate_endpoint_text` already does
rather than write a fourth variant with its own idea of what a control character is.

Header **values** need care: S3 legitimately carries near-arbitrary bytes in some headers, so the rule is
about the framing characters (CR, LF, NUL) that let a value escape its line — not about restricting the
value's alphabet. Say in the code which you chose and why.

## Acceptance criteria

- [ ] A header value containing CR or LF is refused, and a test asserts the canonical request keeps the
      line count it should. Drive it through the public `sign()`, not a helper.
- [ ] `Signer::new` applies the same region check `target_for` does — an empty or control-character region
      is refused on **both** public paths, proven by a test on each.
- [ ] `access_key_id` cannot inject into the `Authorization` header.
- [ ] `validate_bucket` refuses everything `validate_endpoint_text` refuses; a test pins them to the same
      standard so they cannot drift apart again.
- [ ] An empty or whitespace-only `host` value is refused, naming the problem.
- [ ] Each guard broken **on its own** turns a **distinct** test red, with the real output pasted in the
      PR, per the Evidence Rules in `Ticketing/wiki.md`.

## Also from the same UAT (test quality, not behaviour)

- The control-character endpoint loop asserts only `is_err()`, not *which* error fired. The UAT noted a
  related case where the **port** check caught a CRLF endpoint with a message about ports — so `is_err()`
  alone is false comfort. Assert on messages.
- `a_bucket_name_may_not_carry_userinfo_or_a_port` covers only `@` and `:`; the class actually still open
  (item 3 above) has no test.
- `resolved_addressing()` on a malformed endpoint returns `Path` rather than surfacing the error. Harmless
  today because `object_target` still errors, but it is an API that answers confidently when it cannot know.

## Notes

Filed by the Foreman from the PR #868 UAT, 2026-08-12. Not blocking that PR: everything here predates it or
sits beside what it fixed, and it fixed the eight it was asked to.

Related: **CPE-1689** (which closed the endpoint door), **CPE-1684** (the slice that makes this live), and
the recurring rule — *validate every entrance, or the unvalidated one is the one that gets used*.
