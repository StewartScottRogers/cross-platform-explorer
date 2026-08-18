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

## Work Log (2026-08-17)

**Change:** `withoutTrailingSeparator` in `src/lib/tags.ts:115-141` (called only from `keyToWrite`,
`src/lib/tags.ts:105-113`) now strips a trailing **forward slash only**. Removed the `last === "\\"`
branch entirely; the function's last-char check is now `if (p[p.length - 1] !== "/") return p;`.
Updated the function's doc comment to record the CPE-1754 rationale (a trailing backslash is a legal
POSIX filename character, so stripping it collapsed a real file's path onto its sibling and a tag write
landed on the wrong entry).

**Callers audited:** grepped the whole `src/` tree for `withoutTrailingSeparator` — its only two call
sites are both inside `keyToWrite` itself (the exact-key `key = withoutTrailingSeparator(path)` line and
the loop's `withoutTrailingSeparator(k) === key` comparison). No other module imports or calls it, so
the fix did not need narrowing to a "keyToWrite's path only" variant — changing the one function is
sufficient and there is no other caller that could have wanted the backslash case (none exists).

**Root-guard reasoning (why nothing else changes):** the bare-root regex guard (`/^[A-Za-z]:$/.test(rest)`)
already fires for `C:\` under the OLD code too — old code stripped the trailing `\` first (rest = `"C:"`),
then the regex matched and returned the original `p` unchanged. So `C:\` was never actually vulnerable to
this bug; narrowing to forward-slash-only preserves that exact behaviour (now `C:\`'s last char isn't `/`
at all, so it returns unchanged at the very first check — same outcome, shorter path through the code).

**New tests** added to `src/lib/tags.test.ts`, describe block `"keyToWrite — CPE-1754 (a trailing
backslash is a real POSIX filename char, not a separator)"`:

1. `"does NOT redirect a path ending in a literal backslash onto its non-backslash sibling"` — the
   ticket's primary AC. Builds `store = { "/home/x": { tags: ["the-real-directory"], label: "" } }`,
   calls `keyToWrite(store, "/home/x\\")`, and asserts (a) `written !== "/home/x"` with a message that
   names the redirect by name (not a bare string mismatch), then (b) `written === "/home/x\\"`.
2. `"still redirects a genuine trailing FORWARD slash (the CPE-1737 remote-listing-row case)"` —
   `keyToWrite({ "sftp://h/srv/sub": … }, "sftp://h/srv/sub/")` still returns `"sftp://h/srv/sub"`, so the
   fix doesn't regress the case CPE-1737 shipped for.
3. `"preserves bare roots on all spellings…"` — `keyToWrite({}, "/")`, `keyToWrite({}, "C:/")`,
   `keyToWrite({}, "C:\\")` are all returned unchanged (no store, so this exercises the fallback path,
   not the loop — added for completeness against the AC's root-preservation line).
4. `"first-time tagging still returns the caller's path unchanged, including a Windows path"` —
   `keyToWrite({}, "C:\Users\me\new-folder")` and `keyToWrite({}, "/home/new-file")` both return their
   input unchanged.

**Red-proof (test #1 actually bites):** reverted `withoutTrailingSeparator` to the pre-fix form
(`last !== "/" && last !== "\\"` two-branch check) in the worktree, without touching the tests, and ran
`npx vitest run src/lib/tags.test.ts`:

```
 ❯ src/lib/tags.test.ts (20 tests | 1 failed)
   × tags helpers (CPE-636) > keyToWrite — CPE-1754 (a trailing backslash is a real POSIX filename char, not a separator) > does NOT redirect a path ending in a literal backslash onto its non-backslash sibling
     → keyToWrite redirected a path ending in '\' onto its sibling '/home/x' — a POSIX filename's
       trailing backslash is being stripped as if it were a separator: expected true to be false

AssertionError: keyToWrite redirected a path ending in '\' onto its sibling '/home/x' — a POSIX
filename's trailing backslash is being stripped as if it were a separator: expected true to be false
❯ src/lib/tags.test.ts:109:197

 Test Files  1 failed (1)
      Tests  1 failed | 19 passed (20)
```

Exactly the one new test targeting the bug reds, with a message naming the redirect (not a generic
string mismatch), and every other test (including the other 3 new ones, which don't exercise the
backslash branch) stays green — confirming the test bites specifically on the reintroduced bug and
isn't a tautology. Re-applied the fix immediately after and reran: `20 tests | 20 passed`.

**Verification:** `npm run check` — 0 errors, 0 warnings. `npx vitest run` (full suite) — 312 test
files, 4060 tests, all passed.

**Assumptions:** none beyond what the ticket already stated. No backend (Rust) changes needed — this is
frontend-only key selection.
