---
id: CPE-1701
title: Nothing enforces that gui-smoke/lib stays flat, so a nested test file would silently stop being run
type: bug
priority: Medium
status: Done
tags: ready
estimate: XS
created: 2026-08-13
closed: 2026-08-13
---

## Problem

CPE-1694 (PR #878) had to narrow `gui-smoke/package.json`'s `test:unit` glob from `lib/**/*.test.ts` to
`lib/*.test.ts`, because `**` depends on bash's `globstar` — off by default on GitHub Actions' `bash -e`
step shell and unsupported by npm's `sh -c` wrapper. Without the narrowing the step failed on **every** PR
regardless of test outcome. That was the right fix and it is correct today: `gui-smoke/lib/` is flat, four
test files, no subdirectories.

But it leaves a gap. **If anyone later adds `lib/foo/bar.test.ts`, it silently stops being collected** and
the suite reports green having never run it — which is CPE-1694's own bug (*"tests that cannot fail are not
evidence of anything"*) reproduced one directory down. Nothing guards it: no lint or test asserts the
`test:unit` script's content, and no check notices a nested test file.

The reviewer also confirmed there is **no local check that would catch the glob regressing to `**`** either
— only a real CI run does, and nothing but human review currently stops someone restoring it.

### Both gauntlet legs found this independently, and the UAT reproduced it

The UAT created `gui-smoke/lib/nested/thing.test.ts` with a deliberately **failing** assertion and ran the
real `npm run test:unit`:

```
> tsx --test lib/*.test.ts
# tests 71
# suites 24
# pass 71
# fail 0
EXIT CODE: 0
```

Green, exit 0 — the failing test was never collected, and the counts match the pre-probe baseline exactly.

It also checked whether the **pre-PR** `lib/**/*.test.ts` would have done better. It would not. Replayed
under `bash -e -c` with globstar off (matching CI's shell), bash's non-globstar `**` matched **only** the
nested file and silently dropped all 24 flat suites — `tests 1, suites 1, fail 1` instead of 71/24. The
opposite failure mode, equally silent, equally shell-dependent. **The double-star was never a safety net
here; it was differently fragile.** That is worth knowing before anyone proposes restoring it as the fix.

### The portable alternatives were tried and do not work

Recorded so nobody re-derives it:

- **`tsx --test lib`** (directory argument, letting the runner recurse) fails outright:
  `ERR_UNSUPPORTED_DIR_IMPORT`. `tsx --test` does not implement Node's own test-runner directory
  recursion; it treats the argument as a module specifier.
- **Quoting the pattern** to force the runner to glob does not help. The problem is not shell-vs-runner
  priority: `tsx`'s bundled resolver needs the literal `**` to reach it, and on GitHub's runner it still
  fails to resolve even when the shell leaves it unexpanded.

So recursion is not free with this toolchain. A guard is the cheaper answer.

## Scope

`gui-smoke/lib/ratchet.test.ts` (or a small sibling test), and optionally
`gui-smoke/package.json`.

## Acceptance criteria

- [ ] A test fails if a `*.test.ts` file exists anywhere under `gui-smoke/lib/` below the top level. The
      shape the reviewer suggested: compare `readdirSync("lib", { recursive: true })`'s test files against
      `readdirSync("lib")`'s, and red on any difference — so nesting is caught the moment it appears
      rather than when someone notices a test never ran.
- [ ] The failure message says why flatness is required (the `globstar` reason) and what to do about it,
      so whoever hits it does not simply delete the guard.
- [ ] **Prove it**: add a nested `lib/nested/probe.test.ts`, show the guard red with real pasted output,
      remove it, show green. Per the Evidence Rules in `Ticketing/wiki.md`.
- [ ] Decide and record whether the `test:unit` script string itself should also be pinned (so a revert to
      `**` reds locally instead of only in CI). Either answer is fine with the reasoning written down.

## Also worth a comment while you are in there

The reviewer extended `specBasename`'s shape table past the five CPE-1698 fixed and found three
divergences from real `path.basename`, all in the same filter-then-fallback family:

| Input | `specBasename` | real `basename` |
|---|---|---|
| `///` (separators only) | `///` — whole string, fallback fired | `""` |
| `C:x.ts` (drive-relative) | `C:x.ts` | `x.ts` (win32) |
| `C:\` (bare drive root) | `C:` | `""` (win32) |

**None is a real-world risk** — `@wdio/json-reporter`'s `specs[]` entries are always real absolute paths
ending in an actual `.smoke.ts` file, so none of these three shapes can reach the function. Record them as
a known non-goal in a code comment rather than fixing them; the value is that the next reader does not have
to rediscover that the divergence is deliberate.

## Notes

Filed by the Foreman from the PR #878 review, 2026-08-13. The reviewer explicitly declined to block on it:
the risk is currently zero (verified no nested files exist) and the ticket's scope was the CI-breaking bug.

Related: **CPE-1694** (the narrowing that created the gap), **CPE-1698** (`specBasename`), **CPE-1680**
(which moved this code into `lib/`), **CPE-1690** / **CPE-1699** (the same never-run-in-CI shape, one and
two levels out).
