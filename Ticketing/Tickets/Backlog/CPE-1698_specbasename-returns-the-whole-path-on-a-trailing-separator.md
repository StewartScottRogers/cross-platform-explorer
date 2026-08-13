---
id: CPE-1698
title: specBasename returns the entire path when the spec path ends in a separator
type: bug
priority: Low
status: Backlog
tags: ready
estimate: XS
created: 2026-08-12
closed:
---

## Problem

Found by the independent UAT on PR #873 (CPE-1680), which drove five spec-path shapes through the real
`npm run ratchet` CLI. Four resolved correctly; one did not.

`specBasename` in `gui-smoke/lib/ratchet.ts` is a hand-rolled replacement for `path.basename`, written
to keep the lib module import-free when `toCaseStatus`/`reduceResultChunks` moved out of the CLI
wrapper. Its fallback is `parts[parts.length - 1] || specPath`.

`"/home/runner/gui-smoke/specs/case-c.smoke.ts/".split(/[/\\]/)` ends in an empty string, so the `||`
fallback fires and the function returns **the entire original path** instead of a basename.

Verified against real Node: both `path.posix.basename` and `path.win32.basename` strip the trailing
separator and return `"case-c.smoke.ts"`. The code comment explicitly claims to reimplement
`path.basename`, so this is a stated-contract violation, not a judgement call.

| Spec path shape | Result |
|---|---|
| Windows backslash path | correct |
| Mixed `C:/a\b/spec.smoke.ts` | correct |
| **Trailing separator** | **returns the whole path** |
| Bare filename, no separator | correct |
| Directory component containing a dot | correct |

## Why it is Low, not blocking

It fails **loud**, not quiet — the opposite direction from the bug CPE-1680 exists to prevent. In the
UAT's real CLI run the affected case produced a `NEW GUI REGRESSION` (unlisted, because the key was the
full path) *and* a `STALE EXEMPTION` (the real entry going unmatched) simultaneously, exit 1. So the
gate gets noisier, never quieter — it cannot let a regression through.

The UAT also could not reproduce it from real wdio output: `@wdio/json-reporter`'s `specs[]` entries are
file paths, not directories, so nothing observed in CI emits a trailing separator. This is a code-level
divergence found with a crafted fixture, not a production incident.

## Scope

`gui-smoke/lib/ratchet.ts` — `specBasename`.

## Acceptance criteria

- [ ] A spec path with one or more trailing separators resolves to the same basename as
      `path.posix.basename` / `path.win32.basename`. The natural fix is to drop empty segments before
      taking the last one.
- [ ] The four shapes that already work still work — Windows, mixed-separator, bare filename, dotted
      directory component. Pin all five in one test so the set cannot silently shrink.
- [ ] The guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per
      the Evidence Rules in `Ticketing/wiki.md`.
- [ ] If you conclude the import-free constraint is not worth a hand-rolled implementation, using
      `path.basename` again is an acceptable outcome — but say why the constraint no longer applies,
      rather than quietly reintroducing the import.

## Notes

Filed by the Foreman from the PR #873 re-UAT, 2026-08-12. The re-UAT was run specifically because the
code had physically moved between modules after the first UAT sign-off; this is what it found, and the
reviewer's move-audit had not caught it.

Related: **CPE-1680** (which introduced `specBasename`), **CPE-1677** (the gate it serves),
**CPE-1694** (these unit tests do not yet gate CI at all).
