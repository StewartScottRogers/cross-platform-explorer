---
id: CPE-1663
title: The repo-browser's URL guard forwards a Windows path and an ordinary sentence as if they were owner/name
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Found by the independent UAT on PR #851, as a residual **pre-existing** gap — CPE-1650's diff did not touch
it and did not introduce it.

`RepoBrowser.svelte`'s input guard is `!r.includes("/") || looksLikeUrl(r)`, and `looksLikeUrl()` only
recognises two shapes: `scheme://…` and the SCP form `user@host:…`. Anything else containing a `/` is
forwarded to `forge_browse` as though the user had typed `owner/name`. Two inputs a person genuinely types
slip through:

| Input | What happens | What should happen |
|-------|--------------|--------------------|
| `C:/repos/thing` (a Windows path with forward slashes) | forwarded to `forge_browse` | rejected with "Enter a repository as owner/name." |
| `Fix: update src/main.rs docs` (a sentence with a colon and a slash) | forwarded to `forge_browse` | same rejection |

The backslash spelling (`C:\repos\thing`) is already correctly rejected, as are foreign-host SSH URLs, a
sentence with a colon but no slash, and every SSH form CPE-1650 added. So this is a narrow hole, not a
broken guard.

## Why it matters

Low severity — the outcome is a confusing failed lookup rather than anything unsafe (`clone_host()` never
parses a caller-supplied URL, confirmed during the PR #851 review). But "paste a path by mistake, get a
baffling remote error" is exactly the kind of small sharp edge this app's purpose statement says to file
down, and the fix is a few characters of guard.

## Proposed fix

Tighten the *positive* test rather than adding more negative cases: a valid `owner/name` is two non-empty
segments of repo-name characters with exactly one `/` and no colon, no backslash, no whitespace, and no
drive-letter prefix. Reject anything else and let the existing message explain. Prefer one clear predicate
over a growing list of special cases — the current shape is already two exceptions deep.

## Acceptance criteria

- [ ] `C:/repos/thing` and `Fix: update src/main.rs docs` are both rejected with the existing
      "Enter a repository as owner/name." message, and never reach `forge_browse`.
- [ ] Everything CPE-1650 fixed still works: `git@github.com:owner/repo.git`, `ssh://git@host/owner/repo`,
      with and without a port, with and without `.git`, on GitHub and GitLab.
- [ ] A plain `owner/name` still works, including names with dots, dashes and underscores.
- [ ] A foreign-host SSH URL is still rejected when it does not match the selected provider.
- [ ] Tests cover each row above, and removing the new guard turns them red.

## Notes

Filed by the Foreman from the PR #851 UAT, 2026-08-12. The UAT verified all of this by rendering the real
`RepoBrowser` component, typing into the actual input and clicking the real Browse button — not by calling
the parsing helpers directly — so the table above is what the UI genuinely does.
