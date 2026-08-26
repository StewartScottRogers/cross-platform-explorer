---
id: CPE-1902
title: msrvSync.test.ts is not CRLF-safe, so it fails spuriously for every Windows contributor
type: bug
priority: Low
status: Backlog
tags: ready
estimate: XS
created: 2026-08-26
---

## Summary

`src/lib/msrvSync.test.ts` locates the MSRV block in the CI workflow with a raw
`ci.indexOf("\n  msrv:\n")` against the file's text.

On a Windows checkout with `core.autocrlf=true` — the default here, and confirmed on this machine —
`.github/workflows/ci.yml` lands on disk with CRLF endings, that LF-only search misses, and **2 of the
test's 11 cases fail** for reasons that have nothing to do with the MSRV being wrong.

**It is not a real failure.** The committed git blob is plain LF (`git show HEAD:.github/workflows/ci.yml`
confirms), so GitHub's Ubuntu runners — where `npx vitest run` actually executes in CI — see LF and
pass. Normalising the local checkout (`sed -i 's/\r$//'`) makes all 11 pass.

The harm is to the contributor, not the pipeline. Someone on Windows runs `npx vitest run` before
pushing, gets two red tests in a guard about **Rust version floors**, and has no reason to suspect line
endings. The likely reactions are all bad: chase a phantom MSRV problem, assume the suite is flaky, or
learn to ignore red locally. This repo already carries a memory about PowerShell silently re-encoding
files; a guard that reds on encoding rather than on its subject feeds the same confusion.

Found 2026-08-26 by CPE-1855's independent reviewer, which correctly diagnosed it rather than reporting
a false MSRV defect — and said so explicitly instead of letting the noise stand.

## Acceptance criteria

- [ ] Make the search line-ending tolerant — `\r?\n` in the pattern, or normalise the file text once on
      read before any matching. Prefer normalising on read: it fixes every future match in the file
      rather than the one that happened to break.
- [ ] Sweep the rest of the suite for the same shape. Any test that does a raw `indexOf`/`split` on
      `"\n"` over a file read from disk has this bug latent in it; fix the ones you find or list them.
- [ ] Red-proof it the awkward way round: with the fix in place, deliberately give the file CRLF endings
      and confirm the test still **passes**; then break the MSRV for real and confirm it still **fails**.
      Both directions matter — a fix that makes the test tolerant of everything would be worse than the
      bug.
- [ ] Do not "fix" this by adding a `.gitattributes` rule that forces LF on the workflow file and
      leaving the test brittle. That would hide this instance and leave the next one waiting.

## Notes

Related: **CPE-1855** / **CPE-1865** (the MSRV floor and lockfile guards this test belongs to),
[[powershell-rewrites-corrupt-source-files]] (the adjacent encoding hazard on this machine),
[[ci-runs-three-os-backend-matrix]] (why a Windows-only local failure is easy to dismiss and easy to
misread).

Worth noting for whoever picks it up: the CI matrix runs the frontend suite on Ubuntu only, so no
amount of CI green will ever surface this class of defect. It is visible exclusively to a human running
the suite locally on Windows — which is every contributor on this project.
