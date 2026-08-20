---
id: CPE-1794
title: a transient gh failure makes the freshness check file a duplicate issue instead of failing loud
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

In `.github/workflows/ffmpeg-pin-freshness.yml`, the dedupe step looks for an existing open issue with:

```bash
existing=$(gh issue list ... 2>/dev/null || true)
```

If that `gh` call fails transiently — rate limit, 5xx, a network blip — the `|| true` swallows it,
`existing` is empty, and the step falls into the `else` branch and **creates a new issue**. So a
transient API failure produces a duplicate rather than an error.

That is inconsistent with the deliberate design elsewhere in the same file: the HEAD-check step
explicitly routes `000`/403/429/5xx to a distinct "inconclusive" verdict precisely so an infrastructure
failure is never mistaken for a real finding. The dedupe step should follow the same rule.

The blast radius is small — a duplicate issue is noise, not damage — but the failure mode is the one
this workflow family keeps having to fix: a check that reports the wrong thing confidently instead of
saying it does not know.

## What to do

- Distinguish "no matching issue" from "could not ask". Capture `gh issue list`'s exit status
  separately from its output, and on a genuine failure fail the step with a named error rather than
  proceeding to create.
- Match the existing "inconclusive" vocabulary the HEAD-check step already uses, so the two paths read
  the same way.
- Red-proof it by forcing the lookup to fail (e.g. point `GH_TOKEN` at an invalid value for one run, or
  stub the call) and showing the step now fails named instead of filing. The workflow is dispatchable
  with `--ref <branch>`, so this can be proven on a branch before merge — see CPE-1792's Work Log for
  the pattern.
- If you file a test issue while proving it, close it afterwards and say so.

## Notes

Found by the independent reviewer of PR #943 (CPE-1792) while checking for other never-executed paths
in that workflow, 2026-08-19. It is **pre-existing** — introduced with CPE-1763, not by CPE-1792 — and
was explicitly called non-blocking for that PR.

The same reviewer exercised the dedupe *success* path for the first time (reopened issue #942,
re-dispatched, confirmed the workflow commented on the existing issue rather than duplicating it, then
closed #942 again), so the happy path is now verified. This ticket is only about the failure path.

Related: **CPE-1763** (the check), **CPE-1792** (the apostrophe fix), **CPE-1789** (the pin it watches).
