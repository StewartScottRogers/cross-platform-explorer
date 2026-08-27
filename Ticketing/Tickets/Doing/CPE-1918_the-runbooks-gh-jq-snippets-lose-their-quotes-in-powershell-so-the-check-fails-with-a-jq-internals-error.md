---
id: CPE-1918
title: the runbooks' `gh --jq` snippets lose their embedded quotes in PowerShell, so a release check fails with a jq internals error instead of its crafted message
type: bug
priority: Low
status: In Progress
tags: ready
estimate: XS
created: 2026-08-27
---

## Summary

Two runbook snippets tell a human to copy-paste a `gh ... --jq '...select(.name=="x")...'` command
into PowerShell. On Windows PowerShell 5.1 that does not work: PowerShell strips the embedded `"`
characters when marshalling a single-quoted string to a native executable's argv, so `jq` receives
the selector **unquoted** and tries to parse the hyphenated identifier as chained function calls.

Observed verbatim by an independent UAT tester on 2026-08-27, running the `RELEASING.md` snippet
against real run `32645968281`:

    gh : function not defined: sidecar/0

Root cause confirmed with `node -e "console.log(JSON.stringify(process.argv.slice(1)))"`: `jq` is
handed `.jobs[] | select(.name==verify-published-manifest-sidecar)` — bare and unquoted.

## Where

- `RELEASING.md` — the new sidecar publish check (`verify-published-manifest-sidecar`), added by
  CPE-1908.
- `.claude/commands/run.md` line ~55 — the equivalent plain-channel check. **This is the original;
  CPE-1908 copied an existing broken pattern rather than inventing a new one.**

The correct shape already exists two lines above the broken one in `run.md`:
`--jq ".[] | select(...==\"<TAG>\")"` (double-quoted outer, escaped inner).

## Why this is Low and not High

**It fails safe.** `$job` ends up falsy either way, so the following `throw "...do not publish"`
still fires and still blocks the human from publishing. Nothing unsafe gets through. The cost is
purely diagnostic: at 2am the operator sees a confusing jq internals error instead of the crafted
"this job did not pass, do not publish" message, and may waste time debugging `gh` rather than
reading the actual release state.

## Acceptance criteria

- [ ] Fix the quoting in **both** `RELEASING.md` and `.claude/commands/run.md`, using the
      already-correct escaped-double-quote shape that sits two lines away in `run.md`.
- [ ] Actually run each fixed snippet, verbatim, in Windows PowerShell 5.1 against a real run id,
      and paste the output in the Work Log. A runbook fix that is only reasoned about is the same
      class of defect as the bug.
- [ ] Sweep for the same pattern elsewhere — `grep` for `--jq '` across `*.md` and `.claude/` — and
      fix every instance, rather than the two we happen to have tripped over.
- [ ] Consider whether a guard can pin this at all (e.g. a test that extracts fenced PowerShell
      snippets from the runbooks and asserts they contain no single-quoted `--jq` with embedded
      double quotes). If that is not worth the machinery, say so and why.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1039's independent UAT finding. Not a regression in
CPE-1908 — the tester explicitly confirmed the pattern pre-dates it.

Separate observation from the same UAT run, recorded here so it is not lost: `verify-release-artifacts`
prints "…AND that pubkey/endpoints match the second in-repo pin…" **unconditionally**, including on
runs where `--skip-pin-check` bypassed exactly that check. That is a success message claiming a
verification that did not happen. It belongs with **CPE-1901** (`--skip-pin-check` is a one-token kill
switch for the updater pin), not here.

## Work Log

### 2026-08-27 — fixed; the shape the ticket proposed as the fix turned out to be broken too

**The ticket's premise was half wrong, in the direction that matters.** It says the correct shape
"already exists two lines above the broken one in `run.md`": `--jq ".[] | select(...==\"<TAG>\")"`.
Measured on this machine (Windows PowerShell 5.1.26100.9168) with
`node -e "console.log(JSON.stringify(process.argv.slice(1)))"`, that shape is **also broken** — it
just fails differently, which is exactly why the bug survived being "fixed" once and was then copied
forward by CPE-1908.

What each shape actually delivers to `jq`:

| snippet as written | `jq` receives | |
|---|---|---|
| `--jq '.jobs[] \| select(.name=="x")'` | `.jobs[] \| select(.name==x)` | broken |
| `--jq ".jobs[] \| select(.name==\"x\")"` | `.jobs[] \| select(.name==" x\)` | broken, differently |
| ``--jq ".jobs[] \| select(.name==`"x`")"`` | `.jobs[] \| select(.name==x)` | broken |
| `$q = '…"x"…'; --jq $q` | quotes still stripped | a variable does not help |
| `--jq '.jobs'` | `.jobs` | fine |

Verbatim reproduction against the exact real run the ticket names (`32645968281`):

```
=== RELEASING.md snippet verbatim ===
function not defined: sidecar/0
exit=1

=== run.md's "already correct" escaped-double-quote shape ===
failed to parse jq expression (line 1, column 60)
    .jobs[] | select(.name==" verify-published-manifest-sidecar\)
                                                               ^  invalid escape sequence "\)" in string literal
exit=1
```

**The fix is therefore not a better escape** — it is removing string literals from `--jq` entirely.
`--jq` now only plucks the sub-tree (`'.jobs'`, `'.[0].databaseId'`) and the name/tag match is done in
PowerShell via `ConvertFrom-Json` + `Where-Object { $_.field -ceq 'literal' }`. `-ceq`, not `-eq`:
PowerShell's `-eq` is case-insensitive where jq's `==` is not, and these matches gate a publish.

**A second live trap, found only by running it.** In PS 5.1 `ConvertFrom-Json` emits a JSON array as
ONE pipeline object, so `… | ConvertFrom-Json | Where-Object { $_.name -ceq 'create-release' }`
compares the *whole array*; the array comparison is truthy and **all four** jobs pass the filter:

```
job.name = create-release release-sidecar (ubuntu-latest, unix, linux) release-sidecar (windows-latest, ...) ...
```

Assigning to `$jobs` first and piping the variable enumerates it — `match count = 1`. Both snippets
use the two-step form and carry a note saying why.

**Verified fixed, pasted verbatim, against real runs:**

```
# RELEASING.md block, as written
verify-published-manifest-sidecar did not pass (conclusion: ) -- do not publish
    (the crafted message, not `function not defined: sidecar/0`. That job genuinely does not
     exist on run 32645968281, which predates CPE-1908 — so this is the correct verdict.)

# run.md step 1b-ii, as written, with <TAG> = v0.57.69
workflow=release.yml  jobName=verify-published-manifest  runId=32645894722
jobs on run: release (ubuntu-22.04), release (windows-latest), release (macos-latest, --target universal-apple-darwin), catalog
no verify-published-manifest job found on run 32645894722 -- do not publish

# happy path, same block against a job that does exist on that run
match count = 1
catalog did not pass (conclusion: skipped) -- STOP, do not publish this draft
```

Note `runId=32645894722`: `release.yml` has runs for **both** `v0.57.69` and `v0.57.69-sidecar`, and
`-ceq` picked the former, not the substring-containing latter (`32645968177`). The exact-match property
CPE-1908 round 3 R2-4 required survives the rewrite.

### Sweep — every `--jq` / `-q` in the repo, enumerated rather than recalled

| site | shape | verdict |
|---|---|---|
| `.claude/commands/run.md:31` | `--jq '.assets[].name'` | already fine — no `"` in the argument |
| `.claude/commands/run.md` sidecar run lookup | `--jq ".[] \| select(.displayTitle == \"…\")…"` | **broken → fixed** |
| `.claude/commands/run.md` plain run lookup | `--jq ".[] \| select(.headBranch==\"<TAG>\")…"` | **broken → fixed** |
| `.claude/commands/run.md` verify-job lookup | `--jq ".jobs[] \| select(.name==\"$jobName\")"` | **broken → fixed** |
| `RELEASING.md:117` | `--jq '.[0].databaseId'` | already fine — no `"` in the argument |
| `RELEASING.md:119` | `--jq '.jobs[] \| select(.name=="…")'` | **broken → fixed** |
| `.github/workflows/` — `catalog-freshness.yml`, `release-pipeline-watchdog.yml`, `release-sidecar.yml`, `ffmpeg-pin-freshness.yml` | several with embedded `"` | **correct as-is** — every one sits in a `shell: bash` step on a Linux runner, where single quotes are honoured. `release-sidecar.yml`'s one `shell: pwsh` step has no `--jq`. |

So the rule separating working from broken is **not** "single vs double quotes". It is *whether the
argument contains a `"` at all, and which shell the snippet targets*. `docs/**`, `CLAUDE.md` and
`scripts/**` contain no `gh --jq` (`scripts/ci-poll.mjs` spawns `gh` with an argv array, so no shell
quoting is involved).

### Guard

Worth the machinery. `src/lib/runbookJqQuoting.test.ts` (vitest, same class as
`epicsQueueLayout.test.ts`) scans `RELEASING.md`, `CLAUDE.md`, `README.md`, `.claude/commands/**` and
`docs/**` and asserts:

1. no `--jq` / `-q` argument inside a `powershell`/`pwsh`-fenced block contains a `"`;
2. every fenced block containing a `gh` command line carries a language tag — an unlabelled snippet in
   a repo that runs both bash and PowerShell is a trap.

Unit tests pin the detector against all four measured-broken shapes plus the safe ones. Verified it
actually fails: reintroducing the original `RELEASING.md` line makes it report
`RELEASING.md:126  --jq '.jobs[] | select(.name=="verify-published-manifest-sidecar")'`, then pass again
once reverted. Comment lines inside a block are exempt, so the runbooks can quote the broken form while
explaining it.

### Deliberately not done

- The `verify-release-artifacts` unconditional success message noted in Notes above is left for
  CPE-1901, as this ticket directs.
- No attempt to make the snippets work identically in bash and PowerShell. They target PowerShell,
  they say so via their fence tag, and the new guard enforces that every `gh` block declares a shell.
