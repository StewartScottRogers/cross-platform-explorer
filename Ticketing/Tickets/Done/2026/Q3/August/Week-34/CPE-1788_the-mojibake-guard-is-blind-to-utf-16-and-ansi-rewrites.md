---
id: CPE-1788
title: the mojibake guard is blind to UTF-16 and ANSI rewrites — the same PowerShell round-trip, different output encoding
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-19
closed: 2026-08-20
---

## Problem

CPE-1771's guard catches a PowerShell round-trip when the result is still **UTF-8** — the classic
`—` → `â€"` mojibake. Measured by an independent re-review at **89 of 89** corruption shapes caught,
including double- and triple-encoded ones, so the detector itself is sound.

But the same round-trip can land in two other encodings, and both pass the guard silently:

### 1. UTF-16 — skipped as binary

PowerShell 5.1's `>` and `Out-File` default to **UTF-16LE**. A file rewritten that way has NUL bytes
throughout, so `looksBinary` (NUL in the first 4 KB) skips it before any check runs. And
`hasLeadingBom` only matches the UTF-8 BOM `EF BB BF` — it never looks for `FF FE` or `FE FF`.

So the single most common accidental PowerShell rewrite produces a file the guard does not examine at
all.

### 2. ANSI / CP1252 — decoded lossily to zero hits

PowerShell 5.1 `Set-Content`'s default encoding is ANSI. A file rewritten that way is not valid UTF-8,
so `bytes.toString("utf8")` substitutes U+FFFD and `findMojibake` reports **zero** hits on a genuinely
corrupted file.

The code comment at `src/lib/mojibakeGuard.test.ts` previously said a lossy decode "can only ever LOSE a
match, never fabricate one, so it can't turn a clean file into a false failure". The first half is true
and the conclusion is true — it cannot produce a *false failure*. What it does not say is that it can
produce a **false pass**, which is the direction that matters for a guard. That comment has been
corrected to name this gap and point here.

## Why it matters

This is not a new hazard — it is the *same* hazard, in a different shape. The repo's standing rule
exists because a PowerShell write has already broken a release (`86888aed`, "strip the BOM I added to
the manifests"). CPE-1771 exists to make the next occurrence fail loudly. It does, for one of the three
encodings PowerShell can leave behind.

A guard that covers one third of its own root cause while reporting green is worse than an obviously
absent one, because it converts "nobody checked" into "checked and clean".

## What to do

Both are cheap and independent:

- **UTF-16:** before the binary skip, check for a `FF FE` / `FE FF` BOM and report it as a violation in
  its own right (a repo source file should never be UTF-16). Consider also treating a file whose NULs
  fall on a strict alternating pattern as UTF-16 text rather than binary, so its content can be decoded
  and scanned rather than merely rejected.
- **ANSI:** detect the lossy decode instead of ignoring it —
  `Buffer.compare(Buffer.from(text, "utf8"), bytes) !== 0` means the bytes were not valid UTF-8. Report
  that as its own violation kind ("not valid UTF-8"), rather than scanning the U+FFFD-substituted text
  and concluding the file is clean.

Give each its own `kind` so the allowlist stays honest (CPE-1771 already split `mojibake` from `bom` for
exactly this reason), and **red-proof both** by writing a real file through PowerShell's `Out-File` and
`Set-Content` defaults, per the Evidence Rules in `Ticketing/wiki.md` — not by constructing the bytes
synthetically, since the whole point is to catch what that specific tool actually produces.

## Notes

Filed by the Foreman from PR #936's independent re-review, 2026-08-19. The re-review confirmed no
tracked file currently has its first NUL after 4 KB, so the binary heuristic has no present false
negative from that direction — this ticket is about the encodings, not the heuristic's threshold.

Related: **CPE-1771** (the guard this extends), **CPE-1783** (real corruption in `dispatch.rs`),
**CPE-1784** (683 occurrences across `Ticketing/`), and the release that motivated all of them,
`86888aed`.

## Work Log

**2026-08-20** — Implemented both checks in `src/lib/mojibakeGuard.ts` (`detectUtf16Bom`,
`findFirstInvalidUtf8Byte`, plus fixture-simulator helpers `simulatePowerShellAnsiRewrite`/
`simulatePowerShellUtf16Rewrite`) and wired them into `scanOneFile` in `src/lib/mojibakeGuard.test.ts`
(extracted from `scanRepo`'s loop body so the CI walk and the fixture red-proof tests run the identical
per-file check sequence), each with its own `kind` (`"utf16"`, `"not-utf8"`) alongside the existing
`"mojibake"`/`"bom"`.

Deviated from the ticket's suggested `Buffer.compare(Buffer.from(text,"utf8"), bytes) !== 0` approach for
the ANSI check: that gives a boolean, not a byte offset, and rule 4 (fail loudly and specifically) needs
an exact location. Wrote a byte-level UTF-8 well-formedness walker instead (`findFirstInvalidUtf8Byte`)
that returns the 0-based offset of the first invalid byte, which `scanOneFile` turns into a `file:line`
report the same way `findMojibake`'s line tracking does.

Deviated from the ticket's "Consider... a strict alternating NUL pattern" UTF-16 heuristic: skipped it.
PowerShell's `Out-File`/`>` UTF-16LE default always writes the BOM (`Encoding.Unicode`), so the BOM check
alone covers the realistic case the ticket opens with ("the single most common accidental PowerShell
rewrite"); the alternating-NUL heuristic only matters for a deliberate `-NoBOM` write, which is out of
scope for "the same PowerShell round-trip" this ticket is about, and adds false-positive surface (a NUL-
heavy binary that happens to alternate) for no real-world case currently seen in this repo.

Deviated from red-proofing via a real `powershell.exe Out-File`/`Set-Content` run: per the Foreman's
explicit override (this repo has a standing memory note that PowerShell writes have broken a release
here, most recently blocking 0.57.66), fixtures are built with explicit, documented byte-level
constructors (`simulatePowerShellAnsiRewrite`/`simulatePowerShellUtf16Rewrite`) reusing the module's own
CP1252 table, written to disk via node's `fs.writeFileSync` (never PowerShell) in an untracked OS temp
directory, and read back with the same `fs.readFileSync` + `scanOneFile` path the real tree scan uses.
Independently cross-checked the byte sequences against python's `str.encode("cp1252")`/`"utf-16-le")`
during development (see the PR body) rather than trusting the TS-side construction alone.

False-positive sweep: ran both new checks against every git-tracked file outside `Ticketing/` and
`samples/` (3246 tracked, ~3198 in scope) — zero files trip either check. `samples/**` (48 files,
deliberately non-UTF-8/BOM'd fixture corpus per CPE-1042) remains excluded via the existing
`EXCLUDE_PREFIXES`, unchanged; no new exclusion was needed.

Red-proofed all three claims by hand before committing: (1) zeroed out the `utf16` check in `scanOneFile`
— its dedicated fixture test failed (`expected [] to have a length of 1 but got +0`), reverted; (2) same
for the `not-utf8` check, same failure shape, reverted; (3) appended a real CP1252-shape mojibake em-dash
(the original CPE-1771 corruption, generated via python's `.encode("utf-8").decode("cp1252")`, not typed
by hand) to `CLAUDE.md`, ran the tree-wide guard, got `CLAUDE.md:245 [mojibake] -- contains the mojibake
signature "â€”"`, reverted with `git checkout -- CLAUDE.md`. All three reverts confirmed clean via `git
diff --numstat` before proceeding.

Gates: `npx vitest run src/lib/mojibakeGuard.test.ts` — 62/62 passed. Full `npx vitest run` — 319 test
files, 4233 tests, all passed. `npm run check` — 0 errors, 0 warnings.

PR: see branch `cpe-1788-guard-utf16-ansi`.

**2026-08-20 (addendum)** — The Reviewer correctly challenged the first Work Log entry's "per the
Foreman's explicit override" as unverifiable: it cited an authority nobody else could see. Recording
the override with its actual provenance, since being technically correct did not make that phrasing
an acceptable record.

**The override, quoted verbatim**, from the Foreman's dispatch instructions for this ticket:

> "5. Tests must be able to FAIL. Stage genuinely corrupted fixture files (write the bytes explicitly
> with python — do NOT try to produce them through PowerShell, which is the very tool that causes
> this damage and has broken a release here) and assert the guard catches each."

This instruction overrides the ticket's own Evidence Rule above ("red-proof both by writing a real
file through PowerShell's `Out-File` and `Set-Content` defaults... not by constructing the bytes
synthetically"). The Foreman's stated reason: the standing repo guardrail against PowerShell writing
repo files (PowerShell has BOM'd files and blocked a release here before, commit `86888aed`) took
precedence over the ticket's instruction to use the real tool, specifically to avoid producing
corrupted files inside a worktree that might get committed. That is why the implementation fixtures
were built with `simulatePowerShellAnsiRewrite`/`simulatePowerShellUtf16Rewrite` (explicit byte-level
construction) rather than a real `powershell.exe Out-File`/`Set-Content` run.

**Real-invocation evidence now exists from two independent sources**, closing the gap the simulators
left open:

- An independent UAT tester ran the real tool in a throwaway worktree (PowerShell permitted there
  deliberately, since reproducing the actual damage was the point):
  - `Set-Content` with no `-Encoding` on a real tracked file: a real em dash's UTF-8 bytes `E2 80 94`
    collapsed to a single `0x97`; the guard caught it as `README.md:9 [not-utf8] -- invalid byte 0x97
    at offset 395` (line and offset hand-verified).
  - `Out-File -Encoding unicode`: produced `FF FE` + a UTF-16LE body; caught as `README.md:1 [utf16]`.
  - Notable: that tool session sets `$PSDefaultParameterValues['Out-File:Encoding']='utf8'`, which
    MASKS the real default. With that removed, PowerShell 5.1's genuine `>`/`Out-File` default is
    UTF-16LE with a BOM — matching this guard's design assumption.
  - Also honest: `(Get-Content f) | Set-Content f` round-trips byte-identically on that machine,
    because the CP1252 best-fit table maps all 256 byte values back to themselves. There is nothing
    for the guard to catch in that specific round-trip shape, and this PR does not claim otherwise.
  - Both real-PowerShell test files (throwaway UAT worktree) never touched this branch or a commit.
- The Reviewer independently ran `powershell.exe -NoProfile` for both shapes and confirmed they match
  this PR's simulators byte-for-byte, and separately cross-checked the CP1252 fixture against
  Python's `str.encode('cp1252')` — `0x97` at offset 20, exact match to the value reported in the PR
  body.

**Cost.** UAT's machine measured this file's test suite at 62 tests in 1.8-2.7s on this branch,
against 42 tests in 1.9-3.0s on `main` — the Reviewer's own timings were too noisy to be conclusive,
so UAT's numbers are the ones on record.

**Correction to this PR's own framing**: the Reviewer verified that `findFirstInvalidUtf8Byte` scans
the WHOLE file, not just the first 4 KB — `looksBinary`'s 4 KB window only gates whether the
not-valid-UTF-8 check runs at all (a real binary is skipped before reaching it), but once a file is
in scope, every byte to EOF is walked. So a "mostly ASCII with a few high bytes late in the file"
corruption is caught, which is stronger than this PR's description implied.
