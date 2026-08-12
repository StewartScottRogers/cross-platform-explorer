---
id: CPE-1668
title: The repo browser's Clone button bypasses the input guard that Browse now enforces
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Found by the independent Reviewer on PR #852, while checking whether CPE-1663's new guard closed the class
it claims to.

CPE-1663 replaced `browse()`'s two-exceptions-deep negative test with a single positive predicate,
`isRepoId(r)` = `/^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/`. That is correct and now well covered.

But `clone()` in `src/lib/components/RepoBrowser.svelte` (~line 130) still uses its **own, weaker** guard:
`!r.includes("/")`. Clone is reachable directly — a user can paste and click Clone without ever clicking
Browse (an existing test confirms this path) — so a pasted Windows path or a colon-sentence still reaches
`forge_clone` unguarded.

`git blame` puts that line weeks before CPE-1663 (it came in with CPE-435), so this is **pre-existing and
out of scope** for PR #852, whose acceptance criteria were scoped to `forge_browse` specifically. It is filed
because the new `isRepoId` doc comment's "closes the whole class" framing does not hold for this sibling
path, and a claim that outruns the code is the failure mode this crew has corrected three times tonight.

## Why it matters

Low severity — the backend rejects a malformed slug via `is_safe_repo_slug`, so the outcome is a confusing
error rather than anything unsafe. It is filed for consistency: two entry points to the same feature should
not disagree about what a valid repository identifier is.

## Scope

1. Route `clone()` through the same `isRepoId` predicate `browse()` now uses, with the same friendly message.
2. Re-check the `isRepoId` doc comment afterwards and make sure its claim matches what is now true.
3. While there: confirm no third entry point exists with its own third opinion.

## Acceptance criteria

- [ ] `C:/repos/thing`, `C:\repos\thing`, `Fix: update src/main.rs docs`, `git@github.com@evil.com:o/r` and
      a bare `github.com:owner/repo` are all rejected by **Clone** with the same message Browse gives, and
      none reaches `forge_clone`.
- [ ] Every form CPE-1650 fixed still clones: `git@github.com:owner/repo.git`, `ssh://git@host/owner/repo`,
      with and without a port, with and without `.git`.
- [ ] A plain `owner/name` still clones, including names with dots, dashes, underscores and digits.
- [ ] Tests drive the **real component** — type into the input, click the real Clone button, read what the UI
      shows — rather than calling the predicate directly.
- [ ] Removing the new guard turns those tests red.

## Notes

Filed by the Foreman from the PR #852 review, 2026-08-12. Related: **CPE-1663** (the Browse-side fix, merged
in #852) and **CPE-1650** (the SSH host-strip, merged in #851).
