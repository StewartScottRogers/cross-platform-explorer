---
id: CPE-1902
title: msrvSync.test.ts is not CRLF-safe, so it fails spuriously for every Windows contributor
type: bug
priority: Low
status: Done
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

## Independently confirmed by a second leg

CPE-1855's UAT hit this separately from its reviewer, without either seeing the other's report, and
diagnosed it the same way: `.github/workflows/ci.yml` lands with CRLF under `core.autocrlf=true`, the
raw `indexOf("\n  msrv:\n")` returns `-1`, and the test reports the plainly-false
`"ci.yml has no msrv: job at all"` while the job is present and correct.

Two independent legs converging is the strongest signal this run has, so treat the diagnosis as
settled rather than re-deriving it.

The UAT also identified the counter-example that tells you what "fixed" looks like:
`src/lib/lockfileLockedGuard.test.ts` does **not** have this bug, because it splits and trims per line
rather than searching raw text. Copy that shape.

One consequence worth recording: while this stands, CPE-1855's claimed local gate ("5 passed / 0
failed") is **not reproducible on a Windows checkout**. That is not a false claim by its author — it is
true on LF — but it does mean a contributor following the ticket's own instructions gets a different
answer than the ticket reports, which is its own small corrosion of trust.

## Work Log

- Confirmed red on this actual Windows/CRLF checkout before touching anything:
  `npx vitest run src/lib/msrvSync.test.ts` -> 2 of 5 tests failed with the exact false diagnosis
  quoted in the ticket ("ci.yml has no `  msrv:` job at all"), from `msrvJobText()`'s
  `ci.indexOf("\n  msrv:\n")`.
- Fix: normalise `ci.yml`'s text once on read (`.replace(/\r\n/g, "\n")`) inside `msrvJobText()`,
  before any matching — the "prefer normalising on read" option the AC called out, so every match in
  the function is fixed at once rather than patching the one literal that happened to break.
  `declaredRustVersion()`'s `Cargo.toml` regex and `loopedDirs()`'s `[\s\\]+` split were both already
  CRLF-tolerant (neither requires an LF immediately adjacent to non-whitespace content with zero `\r`
  allowed between), so no change was needed there.
- Green after the fix: `npx vitest run src/lib/msrvSync.test.ts` -> 5/5 pass on this same CRLF
  checkout.
- Red-proof, both directions (done after the fix commit, not before):
  1. Forced `ci.yml` to CRLF for the whole file — all 5 tests still **pass** (the fix holds).
  2. Then broke the MSRV for real (mismatched `rust-version` between a manifest and the pinned
     `dtolnay/rust-toolchain@` version) — the relevant test still **fails**, with the real MSRV-drift
     message, not the "no msrv job" false one.
     Full transcript recorded in the PR description / commit message.
- Swept the rest of `src/*.test.ts` for the same shape (raw `indexOf`/regex requiring an LF
  immediately adjacent to real content, over a file read straight off disk):
  - `src/lib/bidiEscape.guard.test.ts` reads `src/docs/03-explorer.md` (confirmed CRLF on this
    checkout, no `.gitattributes` LF pin) and matches `[\s\S]*?(?=\n- \*\*|\n## |$)` — a **lookahead**
    for `\n`, not an exact-adjacency literal. CRLF still contains a literal `\n` character, so the
    lookahead still finds it (it just also swallows the preceding `\r` into the lazy-matched group,
    which the assertions after it don't care about). Ran it standalone: 100% green on this checkout —
    not a live bug.
  - `src/lib/releaseVersionBump.test.ts` reads `CLAUDE.md` and matches
    `/^## Versioning[^\n]*\n([\s\S]*?)\n## /m` — `[^\n]*` absorbs a trailing `\r` as "not a newline"
    before the literal `\n`, so this is CRLF-safe by construction, not by luck. Ran it standalone:
    112/112 green.
  - `src/lib/sprintDispatchAndCiLogGuards.test.ts` reads `.claude/commands/sprint.md` and does
    `SPRINT_MD.indexOf("\n### ", start + 1)` — unlike msrvSync's bug, this literal has nothing after
    `### ` that must be immediately adjacent to a following `\n`; `indexOf` finds the `\n` inside a
    CRLF pair from any preceding character, so it isn't affected either way. Ran it standalone: green.
  - `epicsQueueLayout.test.ts`, `mojibakeGuard.test.ts`, `ffmpegOverrideAutoDispatch.test.ts` already
    use `\r?\n` explicitly; `lockfileLockedGuard.test.ts`, `ciAptGetHardening.test.ts`,
    `releaseHangHardening.test.ts`, `releaseSidecarDownloadBodyGuard.test.ts`,
    `sprintStallControls.test.ts`, `workflowPwshFileEncoding.test.ts` all `split("\n")` /
    `split(/\r?\n/)` then operate per-line (a `\r` just rides along at the end of one line's string,
    harmless to `.trim()`/`.includes()`/`.startsWith()` checks used on those lines).
  - Conclusion: `msrvSync.test.ts` was the only file with the exact-adjacency shape that actually
    breaks under CRLF. No `.gitattributes` LF-pin added — per the AC, that would hide this class of
    bug rather than fix it, and none of the other files need one since they're safe by construction.
- `npm run check` passes.
