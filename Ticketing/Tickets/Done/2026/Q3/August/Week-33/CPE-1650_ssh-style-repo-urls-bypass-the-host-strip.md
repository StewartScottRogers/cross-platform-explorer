---
id: CPE-1650
title: SCP-style SSH repo URLs bypass the repo-browser host strip
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-11
closed:
---

## Problem

CPE-1620 generalised the repo browser's URL shortening so `https://` URLs are stripped per
provider (github/gitlab/bitbucket/codeberg) rather than GitHub-only. The independent review of
PR #837 found a remaining input shape that still slips through:

Pasting an **SCP-style SSH URL** — `git@github.com:owner/repo.git` — into a *named provider*
field is not touched by `stripRepoUrl()` (no `https?://` to match), and `looksLikeUrl()` returns
false (no `://`). The string contains a `/`, so it passes the "looks like owner/name" guard and is
forwarded to `forge_browse` as a malformed repo identifier — reproducing exactly the confusing
not-found failure CPE-1620 set out to remove, via a different input shape.

## Why it was not fixed in CPE-1620

Out of that ticket's stated scope: its acceptance criteria only covered `https://` URLs, and
git-protocol URLs are Generic Git's domain — the four named providers browse over their HTTP APIs,
not over git. A low-probability paste, so it was filed rather than scope-crept into PR #837.

## Acceptance criteria

- [ ] Pasting `git@<host>:owner/repo.git` into a named-provider repo field either resolves to
      `owner/repo` (when the host matches that provider) or produces the same friendly
      "Enter a repository as owner/name." message — never a raw `forge_browse` call with a
      malformed string.
- [ ] Covers the `ssh://git@host/owner/repo.git` form as well as the SCP-style short form.
- [ ] A negative control: a foreign-host SSH URL pasted for the wrong provider must not reach
      `forge_browse`.
- [ ] `npm run check` + vitest green.

## Notes

- Source: independent reviewer finding #2 on PR #837 (CPE-1620), 2026-08-11.
- Relevant code: `src/lib/components/RepoBrowser.svelte` — `PROVIDER_HOSTS`, `stripRepoUrl()`,
  `looksLikeUrl()`; backend counterpart `clone_host()` in `src-tauri/src/lib.rs`.

## Work Log

- 2026-08-11 — Filed by the Foreman from the PR #837 review. Not a blocker for that PR.
