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

- [x] `percent_encode_path` has an in-process unit test covering `#`, `%`, space, emoji and non-ASCII, and it
      runs on all three OS legs.
- [x] A collection DELETE whose retry also returns 3xx reports an error instead of `Ok(())`, with a test.
- [x] The SFTP-recreate wait loop fails loudly on fall-through, matching its sibling.
- [x] The dispatcher's missing-path mapping is applied through one shared helper, its module doc matches the
      code, and a permission-denied traversal is not reported as `NotFound`.
- [x] `assert_list_matches_seeded_set` carries a comment explaining that presence, not set-equality, is
      deliberate.
- [x] The rig's wall-clock is measured after any parallelisation, and the ticket records the real number —
      whether or not it reaches 10 minutes.

## Notes

Filed by the Foreman from the PR #849 review, 2026-08-12.

Worth remembering why the rig earned its keep: it found three genuine client defects that a fake server
written by the same author as the client could never have surfaced — the `#` truncation, the silently
no-op'ing directory delete, and a dispatcher reporting a missing path as an internal error. Items 1 and 2
above are about making sure the *fixes* for two of those are pinned by something that runs everywhere, not
only by the rig that found them.

## Work Log

2026-08-12 — All six items worked on branch `cpe-1673-network-rig-followups`, PR #860. Every behaviour
change (items 1, 2, 4) proven red on a deliberate revert and green on the fix, confirmed locally before
push. Full PR CI run (31609956961) is green on all 11 jobs, including the 3-OS `Server crates` matrix and
the blocking `Network E2E (ubuntu-latest, real servers)` job.

- **Item 1** — `crates/webdav/src/lib.rs`: added `percent_encode_path_escapes_reserved_and_non_ascii_bytes_but_preserves_slashes`
  covering `#`, `%`, space, "café" (non-ASCII), and an emoji. Deliberately broke `percent_encode_path` to
  `path.to_string()`; the new test went red (`"weird#name.txt" != "weird%23name.txt"`); restored, green.
  `cpe-webdav` is now 14/14 (was 12/12) and ran green on all 3 OS legs in this PR's `Server crates` matrix.
- **Item 2** — same file: the DELETE retry now checks the *retried* response's status too and returns an
  `Err` if it's also 3xx. Added `delete_retry_that_also_redirects_is_reported_as_an_error_not_ok` against a
  dedicated fake server that redirects every DELETE. Deliberately reverted the retry-status check; the new
  test went red (`Ok(())` returned instead of an error); restored, green.
- **Item 3** — `.github/workflows/ci.yml`'s "Recreate the SFTP container" wait loop now tracks an `up` flag
  and does `::error` + `exit 1` on timeout, matching the "Wait for the real servers" loop above it. Verified
  by `bash -n` locally (the rig itself is the only real verifier, and this PR's Network E2E job ran the
  step green, unchanged behaviour on the happy path).
- **Item 4** — `crates/server/src/dispatch.rs`: added `domain_path`/`classify_path_error`, applied to
  `list_dir`, `hash_file`, and `text_stats`; updated the module doc. Added
  `hash_file_of_a_missing_path_is_not_found`, `text_stats_of_a_missing_path_is_not_found`, and 3
  `classify_path_error_*` unit tests (including the EACCES-vs-missing distinction, tested via the pure
  classifier rather than real `chmod` — deterministic across OS/privilege level). Deliberately broke
  `classify_path_error` to treat any stat failure as `NotFound`; the EACCES test went red; restored, green.
  Also deliberately broke `hash_file`'s call site back to bare `domain`; its missing-path test went red;
  restored, green. `cpe-server` dispatch module is 12/12 (was 6/6).
- **Item 5** — `crates/vfs/tests/real_server_conformance.rs`: added the presence-vs-set-equality comment on
  `assert_list_matches_seeded_set`. Also did the optional hardening: `extra_test_root` (the FTPS throwaway-CA
  trust hook) is now behind a new `e2e-extra-ca` Cargo feature on `cpe-ftp` (forwarded through `cpe-vfs`),
  off by default — confirmed `src-tauri cargo check` compiles clean with the hook fully absent from the
  shipped app's dependency graph, not merely inert. Both feature modes verified: `cargo test` /
  `cargo clippy --all-targets -D warnings` clean for `crates/ftp` and `crates/vfs` with and without
  `--features e2e-extra-ca`.
- **Item 6** — parallelised the real-server job two ways: (a) the `cpe-server-ref` release build and the
  `crates/vfs` conformance test-binary pre-build now run concurrently (independent standalone crates, no
  shared files); (b) the 4-test conformance suite now runs as 3 concurrent groups by scheme — sftp / webdav
  / (ftp+ftps, kept serial relative to each other since they share one container + fixture directory).
  **Measured wall-clock: baseline 14m01s (PR #849's own run, `11:48:37`→`12:02:38`) → this PR's run
  7m27s (`15:00:49`→`15:08:12`)** — under the 10-minute target. Per-step data from this run: the parallel
  build step now takes 35s (was ~4m each for the release build alone in two independent baseline samples);
  sftp and webdav conformance now finish in under a second each, running alongside the ftp+ftps group,
  which is now the dominant remaining cost at ~300s (unavoidable while it must stay serialised with itself).
  Left uninvestigated: *why* the FTP+FTPS pair alone takes ~5 minutes — that was already true in the
  pre-existing serial run (hidden inside its 5m01s total) and is a distinct question from parallelising the
  test infrastructure, which is what this item asked for. Worth a follow-up ticket if further speedup is
  wanted. Some of the overall improvement is also plausibly attributable to warmer `swatinem/rust-cache`
  state on this run vs. the two cold/lukewarm baseline samples checked before starting — flagged for
  honesty, not something this ticket's changes can fully disentangle without more repeated runs.

PR: https://github.com/StewartScottRogers/cross-platform-explorer/pull/860 (open, not merged — left for the
Foreman). CI run: https://github.com/StewartScottRogers/cross-platform-explorer/actions/runs/31609956961
(all 11 jobs green).
