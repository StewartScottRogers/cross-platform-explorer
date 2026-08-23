---
id: CPE-1794
title: a transient gh failure makes the freshness check file a duplicate issue instead of failing loud
type: bug
priority: Low
status: Doing
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

## Work Log

2026-08-23: Fixed the dedupe step in `.github/workflows/ffmpeg-pin-freshness.yml` ("File an issue
(deduped) so a human sees it"). Replaced:

```bash
existing=$(gh issue list --state open --label "$label" --json number --jq '.[0].number' 2>/dev/null || true)
```

with a shape that captures `gh issue list`'s exit status separately from its output, and on failure
prints `::error::` with the CPE-1794 explanation, dumps stderr, and `exit 1`s **before** reaching either
the create or comment branch:

```bash
list_output="$(mktemp)"
list_err="$(mktemp)"
if gh issue list --state open --label "$label" --json number --jq '.[0].number' \
     > "$list_output" 2>"$list_err"; then
  existing="$(cat "$list_output")"
else
  echo "::error::could not query existing issues (gh issue list failed) -- see below. Not filing a new issue: a lookup failure is not evidence there is no existing one, and creating on that guess is exactly the CPE-1794 defect this workflow avoids." >&2
  cat "$list_err" >&2
  exit 1
fi
```

**Matched `release-pipeline-watchdog.yml`'s shape (PR #1008, `cpe-1872-fix-release-updater-verify`,
not yet merged as of this writing)** — another worker wrote that file the same day specifically to
avoid this exact defect, and its comment block cites CPE-1794 by name. Read it via
`git show origin/cpe-1872-fix-release-updater-verify:.github/workflows/release-pipeline-watchdog.yml`.
One deliberate deviation from it, per the Foreman's brief: that file redirects the lookup's stdout and
stderr into the SAME captured value (`> "$list_output" 2>&1`), which means a `gh` warning on an
otherwise-successful call folds into `$existing` and makes the issue number non-numeric. This fix keeps
stdout and stderr in **separate** temp files (`list_output` / `list_err`) so a warning never corrupts
the parsed issue number — proven in transcript branch 4 below (issue #456 parses correctly even though
the stub `gh` also wrote a stderr warning on that call).

**Assumption:** the ticket's "Match the existing 'inconclusive' vocabulary" instruction was read as
"read the same way" (both are `::error::`-driven fail-loud paths with a clear named reason), not as a
literal requirement to add a third `stale=inconclusive` output variable to this step — the HEAD-check
step's `inconclusive` verdict answers "is the pin stale?" (a decision consumed by later steps), while
this dedupe step answers "did filing/commenting succeed?" and has no downstream step depending on its
outcome (the job's exit code is already correctly 1 either way, and CI's own inconclusive/stale story
lives entirely in the HEAD-check step). Matching vocabulary in the error text and reasoning (paired with
an explicit "not filing a new issue: a lookup failure is not evidence there is no existing one") reads
as satisfying the spirit without inventing plumbing nothing consumes.

### Red-proofing: stubbed `gh`, three (plus one bonus) branches

Per the Foreman's brief, a real `gh` call must never reach this repo while testing. Built a stub `gh`
script + harness under the scratchpad, put its directory FIRST on `PATH` using `/c/Users/...`-style
entries (not `C:/Users/...`, which Windows bash splits at the `:` and silently fails to shadow the real
binary — the documented cause of two earlier agents filing real issues #1013/#1015 today), and asserted
`which gh` resolves to the stub before running anything. Extracted the "File an issue" step's `run:`
block verbatim (via a small YAML-parsing script) into a standalone script exercised with the stub in
4 scenarios:

```
=== which gh ===
/c/.../scratchpad/cpe1794-test/bin/gh
CONFIRMED: which gh resolves to the stub -- safe to proceed.

### Branch 1: lookup fails (transient gh failure)  (GH_STUB_MODE=fail)
::error::could not query existing issues (gh issue list failed) -- see below. Not filing a new issue: a lookup failure is not evidence there is no existing one, and creating on that guess is exactly the CPE-1794 defect this workflow avoids.
gh: (STUB) simulated transient failure: HTTP 500 (rate limited)
--- exit code: 1 ---
--- gh subcommands invoked (in order) ---
label create dep-pin-stale --color d93f0b --description A pinned native-dep download (ffmpeg/pdfium) in release-sidecar.yml has gone stale
issue list --state open --label dep-pin-stale --json number --jq .[0].number
--- gh issue create calls: 0 ---
--- gh issue comment calls: 0 ---

### Branch 2: lookup succeeds, no open issue  (GH_STUB_MODE=nomatch)
::error::pinned asset(s) unreachable -- see the filed/updated issue above. This run is intentionally red until the pin is fixed.
--- exit code: 1 ---
--- gh subcommands invoked (in order) ---
label create dep-pin-stale ...
issue list --state open --label dep-pin-stale --json number --jq .[0].number
issue create --title ffmpeg/pdfium pin has gone stale in release-sidecar.yml --label dep-pin-stale --body-file /tmp/tmp.bSFTAPXBB1
--- gh issue create calls: 1 ---
--- gh issue comment calls: 0 ---

### Branch 3: lookup succeeds, open issue exists (#123)  (GH_STUB_MODE=match)
an open issue already tracks this (#123) -- commenting with current findings instead of filing a duplicate
::error::pinned asset(s) unreachable -- see the filed/updated issue above. This run is intentionally red until the pin is fixed.
--- exit code: 1 ---
--- gh subcommands invoked (in order) ---
label create dep-pin-stale ...
issue list --state open --label dep-pin-stale --json number --jq .[0].number
issue comment 123 --body-file /tmp/tmp.mExUnTckPX
--- gh issue create calls: 0 ---
--- gh issue comment calls: 1 ---

### Branch 4 (bonus): lookup succeeds WITH a stderr warning, open issue #456  (GH_STUB_MODE=warn)
an open issue already tracks this (#456) -- commenting with current findings instead of filing a duplicate
::error::pinned asset(s) unreachable -- see the filed/updated issue above. This run is intentionally red until the pin is fixed.
--- exit code: 1 ---
--- gh subcommands invoked (in order) ---
label create dep-pin-stale ...
issue list --state open --label dep-pin-stale --json number --jq .[0].number
issue comment 456 --body-file /tmp/tmp.45OSd1NPRc
--- gh issue create calls: 0 ---
--- gh issue comment calls: 1 ---
```

Branch 1 is the whole ticket: zero `issue create` and zero `issue comment` calls when the lookup fails.
Branches 2/3 show the pre-existing happy paths (create when no match, comment when a match exists) are
unchanged. Branch 4 is bonus proof the stderr-separation nit fix works: `$existing` still parses to a
clean `456` even though the same `gh` call also wrote a stderr warning.

No real `gh` command reached GitHub at any point in this work — every invocation above hit the stub.
No test issue was ever filed against the real repo, so there is nothing to close.

Validated `bash -n` clean on all six `run:` blocks in the file (including the changed one) and that the
YAML parses, via a small PyYAML-based script (the file is CRLF on disk due to this repo's
`core.autocrlf=true`; normalized to LF before the syntax check, matching what `git show`/GitHub Actions
actually execute).
