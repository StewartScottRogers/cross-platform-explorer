---
id: CPE-1682
title: "S3 error responses must name the real cause, not a bare status code"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [needs-prereq]
epic: CPE-1503
estimate: S
created: 2026-08-12
closed: 2026-08-13
---

## What

One shared response path in `cpe-s3` that turns a non-2xx S3 response into an error naming what actually
went wrong, before either provider ticket (CPE-1683, CPE-1684) writes a line of error handling of its own.

## Why this is its own ticket, filed before the code it guards

S3 answers a misconfiguration with an HTTP status and an XML body:

```xml
<Error><Code>SignatureDoesNotMatch</Code><Message>…</Message><RequestId>…</RequestId></Error>
```

A provider that reports `s3: HTTP 403` collapses **SignatureDoesNotMatch** (the clock is skewed, or the
secret is wrong), **AccessDenied** (the credential is fine, the policy is not) and **InvalidAccessKeyId**
(the key does not exist) into one indistinguishable string — and the user is left guessing which of three
unrelated fixes to try. That is exactly the "confident answer with the wrong cause" shape this crew keeps
filing tickets against, most recently CPE-1678. Building it once, up front, is cheaper than letting two
tickets each invent their own handling and then reconciling them.

It goes second, right after the foundation, so CPE-1683 and CPE-1684 call it rather than route around it.

## Scope

- Parse the S3 error body into `{ code, message }` and surface the code in the error text. `NoSuchBucket`,
  `NoSuchKey`, `AccessDenied`, `InvalidAccessKeyId`, `SignatureDoesNotMatch` at minimum; unknown codes pass
  through verbatim rather than being flattened.
- A response with **no** parseable error body (a proxy's HTML 502, an empty body, a truncated read) must
  still produce an honest error that says so, and must not claim a code it did not read.
- **Bound and guard the parse.** `cpe-webdav` had to add an element-nesting-depth guard ahead of
  `roxmltree` (CPE-1398) because a hostile deeply-nested document can stack-overflow the process, and a
  hand-rolled byte scan for it turned out to have a quote-unaware evasion bug. Reuse that lesson: cap the
  bytes read from an error body, and depth-guard before parsing. An error path is still an attack surface.

## Acceptance criteria

- [ ] Each of the five named codes produces a distinct error mentioning that code, tested through a
      fixture serving the real S3 XML shape.
- [ ] A 403 with an unparseable body reports the status and says the body could not be read — it does not
      guess a code.
- [ ] An oversized or deeply-nested error body is refused without panicking or exhausting memory, with a
      test that would stack-overflow the process if the guard were removed.
- [ ] Both CPE-1683 and CPE-1684 route their non-2xx responses through this one function — grep proves
      there is no second error-mapping site in the crate.
- [ ] Replacing the parsed code with the bare status turns a test red.

## Notes

Filed by the sprint PM at the CPE-1503 activation, 2026-08-12. Prereq: **CPE-1681** (the crate and its
request path must exist). Prereq for the error-handling half of CPE-1683 and CPE-1684.

Related: **CPE-1398** (the WebDAV XML depth guard whose lesson this reuses) and **CPE-1678** (the same
wrong-cause family, one layer up).
