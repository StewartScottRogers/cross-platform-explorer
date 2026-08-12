---
id: CPE-1673
title: Network E2E rig follow-ups — the WebDAV fixes have no in-process test, the DELETE retry can still no-op, and one wait loop still falls through
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-12
closed:
---

## Problem

Nine non-blocking findings from the independent review of PR #849 (CPE-1659), which merged the real-server
network E2E rig. That PR is good work — the reviewer independently verified the negative control against the
GitHub API and confirmed all three client bugs it found are real — and none of these blocked it. They are
collected here so they do not evaporate.

Ordered by what actually matters.

### 1. The two shipped WebDAV fixes have zero in-process tests (the cheapest win here)

`crates/webdav/src/lib.rs:158` and `:313` fixed two **real, shipped client bugs**: URL paths were unencoded
so a literal `#` truncated the request, and a collection `DELETE` silently no-op'd because Apache
301-redirects it. Both are covered **only** by the Linux-Docker-only rig job. `crates/webdav` is still 12/12,
identical to `main`.

`percent_encode_path` is a pure function. A five-line unit test pins it on the whole 3-OS matrix for free,
and the regression it guards against is one we have already shipped once.

### 2. The DELETE retry can still silently no-op — the same class as the bug it fixes

`crates/webdav/src/lib.rs:158`: the retry against the RFC 4918 trailing-slash convention does **not** check
the *retried* response's status. A second 3xx therefore returns `Ok(())` having deleted nothing — exactly the
failure mode the fix exists to close, one hop further along. Check the retried status and surface a non-2xx.

### 3. One wait loop still falls through silently

`.github/workflows/ci.yml:613` — the "Recreate the SFTP container" loop still exits its retry without
`exit 1`, the precise pattern fixed one step above (and the pattern that hid a vsftpd startup crash for two
full CI runs). Harmless **today** only because the TOFU test asserts `err.contains("CHANGED")` rather than
merely "connect failed", so a dead container fails loudly instead of false-passing. Fix it for symmetry
before someone loosens that assertion and re-creates the hole.

### 4. The dispatcher's error taxonomy is now one-off

`crates/server/src/dispatch.rs`:
- `:7` — the module doc still says a domain `Err(String)` maps to `ErrorCode::Internal`. That is now false
  for `list_dir`.
- `:108` — `NotFound` is special-cased for `list_dir` **only**; `hash_file`, `text_stats` and the rest still
  flatten a missing path to `Internal`. A shared `domain_path(&path, err)` helper would make the taxonomy
  consistent instead of a one-off.
- `:108` — `Path::exists()` returns `false` on a permission-denied parent traversal, so an EACCES can be
  reported as `NotFound`. "We don't know" reported as "it isn't there" is the shape this crew keeps filing.

### 5. Assertion strength and hardening

- `crates/vfs/tests/real_server_conformance.rs`, `assert_list_matches_seeded_set` asserts **presence**, not
  set-equality, so a server returning extra entries passes. Deliberate (FTP and FTPS share a fixture dir) —
  add the comment saying so.
- Optional: put `extra_test_root` behind a Cargo feature the CI job enables, keeping the env-var trust hook
  out of the shipped binary entirely. It already meets the bar (env-gated, inert when unset, unit-tested);
  this is defence in depth only.

### 6. The job takes 14 minutes against a stated 10-minute target

Honestly marked `[~]` in the ticket rather than falsely ticked, with the cause named: `--test-threads=1`
serialisation plus a release build plus container startups. No follow-up was filed, so it is filed here.
Worth attacking by parallelising the conformance suite across schemes (they use separate containers and
separate fixture roots) and caching the release build.

## Acceptance criteria

- [ ] `percent_encode_path` has an in-process unit test covering `#`, `%`, space, emoji and non-ASCII, and it
      runs on all three OS legs.
- [ ] A collection DELETE whose retry also returns 3xx reports an error instead of `Ok(())`, with a test.
- [ ] The SFTP-recreate wait loop fails loudly on fall-through, matching its sibling.
- [ ] The dispatcher's missing-path mapping is applied through one shared helper, its module doc matches the
      code, and a permission-denied traversal is not reported as `NotFound`.
- [ ] `assert_list_matches_seeded_set` carries a comment explaining that presence, not set-equality, is
      deliberate.
- [ ] The rig's wall-clock is measured after any parallelisation, and the ticket records the real number —
      whether or not it reaches 10 minutes.

## Notes

Filed by the Foreman from the PR #849 review, 2026-08-12.

Worth remembering why the rig earned its keep: it found three genuine client defects that a fake server
written by the same author as the client could never have surfaced — the `#` truncation, the silently
no-op'ing directory delete, and a dispatcher reporting a missing path as an internal error. Items 1 and 2
above are about making sure the *fixes* for two of those are pinned by something that runs everywhere, not
only by the rig that found them.
