---
id: CPE-1722
title: "Provider paths collapse leading/trailing slashes one layer above `object_target`, so `//a.txt` writes `a.txt`"
type: Bug
status: Done
priority: Medium
component: Backend
tags: [ready]
epic: CPE-1503
estimate: M
created: 2026-08-13
closed: 2026-08-20
---

## What

CPE-1689 established that a leading slash is **part of an S3 key**: `a.txt`, `/a.txt` and `//a.txt` are
three different objects at three different URLs, and `S3Config::object_target` implements that correctly —
it appends the key to the bucket root verbatim and refuses to trim.

But `cpe_s3::provider`'s path→key helpers sit **one layer above** it and do
`path.trim_matches('/')`, which strips every leading *and* trailing slash before `object_target` is ever
reached. So the guarantee is correct at the layer that documents it and lost at the layer callers use.

Measured by the CPE-1684 round-2 UAT:

```text
UAT-B2 provider path "//a.txt"   -> wire Some("/test-bucket/a.txt")
UAT-B2 provider path "/a.txt/"   -> wire Some("/test-bucket/a.txt")
UAT-B2 object_target("/a.txt")   -> Ok("/test-bucket//a.txt")
```

`write("//report.pdf", …)` therefore **overwrites `report.pdf`** — precisely the silently-wrong-object
failure CPE-1689 exists to prevent, arriving one layer up from where that ticket looked.

## Reachability, stated honestly

**Hand-typed (or programmatically-constructed) paths only.** A path reached by navigating a listing cannot
contain the shape: `is_safe_s3_leaf` refuses any leaf containing `/`, so no `ProviderEntry` can produce an
empty path segment. That is why this is Medium and not a blocker — but "you have to type it" is exactly
the case CPE-1689 called out, since a bucket written by a tool that joined paths carelessly genuinely
holds all three keys and a user pasting one in is the person most likely to need it.

## Why CPE-1684 did not just fix it

Recorded so this is a decision with reasons rather than a deferral:

1. **`provider_path_to_key_prefix` has the identical `trim_matches('/')`** and is merged (CPE-1683),
   feeding both `list` and `mkdir`. Fixing only the object-key helper would leave `list`/`mkdir`
   collapsing while `stat`/`read`/`write`/`delete` did not — two conventions inside one provider is worse
   than one documented convention.
2. **Trailing-slash insignificance is a cross-backend contract.** Every other `FileSystemProvider`
   (local, SFTP, WebDAV, FTP) treats `/a` and `/a/` as the same path, and `stat`/`delete` build their
   prefix probe as `format!("{key}/")`. Making S3's path grammar alone slash-significant is a trait-level
   question, not a one-line edit.
3. It is a behaviour change to merged code with no test coverage either way, landing in a PR whose own
   subject was something else.

## Acceptance criteria

- [ ] Decide the path grammar **once, for the whole crate**: either provider paths are byte-exact S3 keys
      (and `provider_path_to_key_prefix` changes in lockstep, with `list`/`mkdir`/`stat`/`delete` all
      agreeing), or they are not (and the limitation is documented as final).
- [ ] Whichever way: a test drives the chosen behaviour **to the wire** through the request-line recorder
      already in `crates/s3/src/provider.rs`'s tests, asserting the sent path — not merely that a helper
      returns a string.
- [ ] `src/docs/31-network.md`'s "Keys are taken literally, including the slashes" bullet is brought back
      into agreement with the code. CPE-1684 weakened it to match reality; if this ticket restores the
      stronger behaviour, the bullet goes back.
- [ ] If the collapse is kept, `provider_path_to_object_key`'s doc keeps saying so plainly rather than
      leaving the CPE-1689 reference implying a guarantee the layer does not provide.

## Notes

Filed 2026-08-13 by the CPE-1684 worker from the round-2 UAT's measurement. Related: **CPE-1689** (which
established the rule and fixed it inside `object_target`), **CPE-1683** (which introduced the sibling
`provider_path_to_key_prefix`), **CPE-1721** (the other place `crates/s3` cannot address a legal key —
there because `ureq` rewrites the path, here because we do).

## Work Log

- 2026-08-20 — merged as **#955** (`caaba863`), batch 30 of the batched sprint.
  A single `rooted_key_bytes` boundary now owns path->key translation, so an S3 key is treated as
  the opaque byte string it is rather than a filesystem path. `stat` no longer mis-derives the
  display name for a trailing-slash key.
- The ureq-2 normalisation half was **measured, not assumed** — a raw-socket probe observed what
  actually goes on the wire across two major versions, and proved no encoding of `.` survives both
  `url` and S3. That measurement is recorded in the module doc and escalated as **CPE-1800**
  (migrate to ureq 3, or accept the limitation) rather than decided inside a sprint slot.
- A slashes-only key found in passing was deliberately **not** fixed in-flight; the current
  behaviour was pinned at `0` so it cannot change accidentally, and filed as **CPE-1801**.
