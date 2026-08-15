---
id: CPE-1754
title: keyToWrite still strips a trailing backslash, so a POSIX name ending in one collapses onto its sibling
type: bug
priority: Low
status: Backlog
tags: ready
estimate: XS
created: 2026-08-15
closed:
---

## Problem

Residual noted by the PR #908 (CPE-1737) round-4 review, which approved the PR and explicitly judged this
not worth holding the merge for.

`withoutTrailingSeparator` (`src/lib/tags.ts:115-131`) strips one trailing `/` **or** `\`. On Linux and
macOS a backslash is a legal filename character, so a name whose **final** character is a backslash
collapses onto its sibling:

```
store = { "/home/x": { tags: ["the-real-directory"] } }
keyToWrite(store, "/home/x\\")  ->  "/home/x"      // still redirects
```

`keyToWrite` selects the key a tag write **lands on**, so a redirect here overwrites another entry's tags
rather than merely mis-displaying something.

## Why it is Low, not a blocker

This is orders of magnitude narrower than the defect it replaced. Round 3 used `canonicalPath`, which
rewrites **every** `\` to `/` anywhere in the path — so `/home/me/a\b` and `/home/me/a/b` collided, a shape
that occurs whenever any component of a name contains a backslash. The shipped form only collides when the
backslash is the **final** character of the whole path. The current behaviour is a strict improvement; this
is the last millimetre.

## The fix

Strip a trailing **forward slash only**. A trailing forward slash is the sole spelling difference CPE-1737
introduces (a remote directory's listing-row path), and the existing root guard already covers Windows
`C:\` without needing the backslash case at all.

## Acceptance criteria

- [ ] `keyToWrite({ "/home/x": … }, "/home/x\\")` returns `"/home/x\\"` — no redirect.
- [ ] The trailing-forward-slash case CPE-1737 needs still redirects:
      `keyToWrite({ "sftp://h/srv/sub": … }, "sftp://h/srv/sub/")` returns `"sftp://h/srv/sub"`.
- [ ] Roots are still preserved: `C:\`, `C:/` and `/` are not stripped down to `C:` or `""`.
- [ ] First-time tagging still returns the caller's path unchanged, including a Windows path.
- [ ] Breaking the guard reds a **distinct** test whose message names the redirect, not merely a mismatched
      string.

## Notes

Related: CPE-1737 (PR #908 — the ticket this fell out of; see its round-3 review for the wider version of
this collision and why `canonicalPath` must never select a write key), CPE-1748.
