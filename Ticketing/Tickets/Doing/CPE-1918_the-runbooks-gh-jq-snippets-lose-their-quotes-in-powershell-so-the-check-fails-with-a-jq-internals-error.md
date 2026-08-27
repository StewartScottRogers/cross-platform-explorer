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

- [x] Fix the quoting in **both** `RELEASING.md` and `.claude/commands/run.md`, ~~using the
      already-correct escaped-double-quote shape that sits two lines away in `run.md`~~.
      **⚠ This AC's prescribed shape was deliberately NOT used, because it is wrong.** Both files are
      fixed, but not that way: the "already-correct" `--jq ".[] | select(...==\"<TAG>\")"` form was
      measured on PS 5.1 and is **also broken** — it delivers `.jobs[] | select(.name==" x\)` and dies
      with `invalid escape sequence "\)"`. Following this AC literally would have shipped a second
      broken snippet while closing the ticket green. The fix removes string literals from `--jq`
      entirely instead. Independently reproduced by the reviewer; see the Work Log.
- [x] Actually run each fixed snippet, verbatim, in Windows PowerShell 5.1 against a real run id,
      and paste the output in the Work Log. A runbook fix that is only reasoned about is the same
      class of defect as the bug. — done against runs `32645968281` and `32645894722`; the reviewer
      re-ran each snippet extracted with `sed` straight from the files.
- [x] Sweep for the same pattern elsewhere — `grep` for `--jq '` across `*.md` and `.claude/` — and
      fix every instance, rather than the two we happen to have tripped over. — full table in the
      Work Log; enumeration independently re-derived by the reviewer and confirmed complete.
- [x] Consider whether a guard can pin this at all (e.g. a test that extracts fenced PowerShell
      snippets from the runbooks and asserts they contain no single-quoted `--jq` with embedded
      double quotes). If that is not worth the machinery, say so and why. — worth it and built:
      `src/lib/runbookJqQuoting.test.ts`. Review found six evasions in the first cut; all six are
      fixed and each is now a fixture in that file.

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
| `.github/workflows/ffmpeg-pin-freshness.yml:365` | `-q '[.[] \| select(.tag_name \| startswith("autobuild"))]…'` | **correct as-is** — the *only* workflow filter with an embedded quote, and it runs in a `shell: bash` step on `ubuntu-latest`, where single quotes are honoured |
| `.github/workflows/` — `catalog-freshness.yml:256`, `release-pipeline-watchdog.yml:90`, `release-sidecar.yml:88`, `ffmpeg-pin-freshness.yml:382,386,582` | quote-free filters (`'.[0].number'`, `'.databaseId'`, `'.assets[].name'`, `'.tag_name'`) | **correct as-is** — nothing to strip |

*(Corrected in review: the first version of this table said "several with embedded `"`". Exactly one
does. Also, **both** `release.yml:116` and `release-sidecar.yml:557` have a `shell: pwsh` step — the
Windows cert-signing step in each — not just `release-sidecar.yml`. Neither has a `--jq`, so the
conclusion is unchanged, but the claim now matches what is actually there.)*

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
actually fails: reintroducing the original line into `RELEASING.md` makes it report the file, the line
and the offending text —

```
RELEASING.md:<line>  --jq '.jobs[] | select(.name=="verify-published-manifest-sidecar")'
```

— then pass again once reverted. *(This citation deliberately no longer hard-codes the line number.
The first draft said `:126`; the reviewer's independent reintroduction reported `:125`. Both were
right — the number moves with every edit above it, so quoting one in a Work Log is a claim that goes
stale on the next commit. The offending text is the durable part.)* Comment lines inside a block are
exempt, so the runbooks can quote the broken form while explaining it.

### 2026-08-27 (round 2) — six evasions in the guard, found in review, all fixed here

The review confirmed every claim above on real PowerShell 5.1 against real runs, and then found that
the *guard itself* had holes. All six are fixed in this PR rather than filed, and **each is now a
fixture in `runbookJqQuoting.test.ts`** — a guard's known evasions belong in its own test table, not in
a review transcript.

### RETRACTED (round 3): the `ConvertFrom-Json` trap is fail-SAFE, not fail-open

**This Work Log, `RELEASING.md` and `run.md` all previously said the broken form makes the publish
check *pass*, "exactly wrong on the cancelled/partial-matrix run". That was wrong, and it was mine —
the reviewer raised a version of it, I sharpened it into bold text in two runbooks, and neither of us
had executed the scenario the claim named.** Struck and replaced everywhere.

What was actually measured, round 1, was a run where **all four jobs succeeded** and the searched-for
job was one of them. That is a *correct* pass. Generalising it to "the check then passes" was an
inference, not a measurement.

Re-measured on real PS 5.1 with controlled job arrays, TRAP = pipe-into-`Where-Object`,
FIXED = assign-then-pipe:

```
S1 partial matrix (target SKIPPED, a leg CANCELLED) -- the case the doc named
   TRAP : matched=4  conc -ne success=True  => THROWS
   FIXED: matched=1  conc -ne success=True  => THROWS
S2 target SUCCESS, sibling FAILED
   TRAP : matched=3  conc -ne success=True  => THROWS
   FIXED: matched=1  conc -ne success=False => PASSES
S3 all success  => both PASS
S4 target absent => both THROW
S5 target failed => both THROW
```

The mechanism the doc stated was **backwards**. `-ne` on an array is a *filter*, not a boolean:

```
@('success','failure') -ne 'success'  ->  failure   [bool]=True
@('success','success') -ne 'success'  ->  ''        [bool]=False
```

So `$job.conclusion -ne "success"` is false only when **every** conclusion is `success` — not "as long
as any job succeeded". Three consequences:

1. **S1, the scenario both runbooks named, THROWS under the trap** — identically to the correct form.
   The partial-matrix run is not a case the trap waves through.
2. **The trap can never pass where the correct form throws.** Passing requires every conclusion to be
   `success`, which entails the target's own.
3. Its real failure mode is the **opposite**: S2, where it *refuses* a publish the correct form
   allows, with a garbled message. Measured:
   `(conclusion: success success cancelled skipped)` instead of `(conclusion: skipped)`.

A genuine bug — wrong answer, unreadable message — but **fail-safe**. The fix stands; only the
justification was wrong.

**Note what this was: a doc asserting a mechanism nobody had executed — inside the PR whose entire
subject is snippets that were never run.** That is CPE-1933's shape exactly, one level up from the
original bug, and it survived a round of review before being caught. The lesson generalises past
snippets: a *rule* stated in a runbook is as much a provenance claim as a command in one, and
"reasoned from how PowerShell works" is the same evidence class as "it looks right".

**F1 — a `--jq` whose filter sits on the PowerShell continuation line evaded entirely.** The most
realistic reintroduction shape, because the shipped snippets already break immediately *before*
`--jq`; moving the break one token later reintroduces the bug with CI green. `logicalLines()` now
folds backtick-continued lines before scanning and reports against the line the command *starts* on.
Red-proofed on the real file: `RELEASING.md:126  --jq '.jobs[] | select(.name=="…")'`.

**F2 — the `--jq=FILTER` equals form evaded** (the old regex required whitespace after the flag).
Measured: `--jq='…"x"…'` delivers `--jq=.jobs[] | select(.name==x)`, broken the same way. The matcher
now accepts `--jq X`, `--jq=X`, `-q X`, `-q=X` and `-qX`, with a lookahead so `-quiet` is not read as
`-q` + `uiet` (there is a real `hdiutil detach -quiet` in `run.md`). Red-proofed on the real file.

**F3 — "names the shell" was only enforced as "has *an* info string".** A PowerShell snippet
mislabelled ```` ```console ```` or ```` ```text ```` was doubly invisible: skipped by the quote check
*and* accepted by the tag check. The tag is now validated against a set that actually names a shell
(`powershell`/`pwsh`/`ps1`/`ps`, `bash`/`sh`/`zsh`); `console`, `text`, `shell` and "" all fail.
Red-proofed on the real file: `RELEASING.md:116  lang=console`.

**F4 — indented and 4-backtick fences were invisible.** The old `/^```(\S*)\s*$/` was column-0 only.
This was **not** latent: re-parsing the real files showed **seven** blocks the guard had never seen —
`README.md:145` and `:209` (both `bash`), `.claude/commands/sprint.md:692`,
`ticketing-organize.md:87`, `skills-organise.md:71` and `:128`, and
`docs/design/VAULT-SECURITY.md:308` (the 4-space-indented untagged pair at 307/309). *(That last one
was missing from the first version of this list, which said "seven" and then named six — corrected in
review. Verified: none of the seven contains a `gh` command or a jq flag, so none was hiding a live
bug; the hole was in coverage, not in the tree.)* `run.md` is a numbered-step document,
so the next `gh` snippet added inside a list item would have been unguarded. The parser now handles
leading indentation and fences of 3+ backticks (a closing fence must be at least as long as its
opener, so a block can contain a ``` line of its own — previously that mis-parsed the language as
`` `powershell ``). Red-proofed on the real file with the block indented three spaces.

**F5 — sweep-table accuracy.** Corrected in place above.

**F6 — the stated rule is over-strict, not exact.** Measured: `--jq '…\"x\"…'` and `--jq '…""x""…'`
(single-quoted, backslash- or doubled-escaped inner) both deliver **intact**. Erring conservative is
right for a guard, but `RELEASING.md` presented "no `"` at all" as the *mechanism*, so a future reader
who found the backslash form working would have had grounds to "correct" the doc — the
provenance-claim shape (CPE-1933). Both shapes are now rows in the doc's table marked *✓ but banned
anyway*, with a clause saying the rule is deliberately stricter than necessary and why: what separates
them from the four broken rows is one character in a position no reader can check by eye, and this
class already regressed once.

**F7 — ticket hygiene.** ACs ticked, with AC #1 explicitly annotated as *not followed, because it was
disproved*. Line-number citation replaced with the offending text.

### Known gaps in the guard, recorded rather than fixed

All four are now one-line entries in `runbookJqQuoting.test.ts`'s header, so the next reader sees the
guard's limits next to its rules rather than having to rediscover them:

1. **Blockquoted fences evade** (`> ```powershell`). None today, but `RELEASING.md` already uses
   blockquote callouts — including the `$jobs` note rewritten above. Stripping a `> ` prefix is easy;
   doing it without breaking a fence that interleaves with a quote boundary is not, so it is left.
2. **`--jq $q` is a miss** — the quotes live on the assignment line, not the flag. That is **row 4 of
   this ticket's own measured-broken table** ("a variable does not help"), so the doc names a broken
   shape the guard cannot pin. Acknowledged explicitly; the doc is the only control for that one.
3. **`Select-String … -q "z"` would be a false red** (PowerShell accepts `-q` as a unique prefix of
   `-Quiet`). None today; if one appears, exempt `Select-String` rather than loosen the flag matcher.
4. **Tilde fences** were a gap and are now **fixed** — it cost one character class. A fence opens on
   three or more backticks *or* tildes and closes only on the **same** character, so a `~~~` block is
   not terminated by a ``` line. Two fixtures added (tilde block; and `` `` `` is not a fence).

### A second escaping trap, hit while making this very fix

Worth recording because it is the ticket's own subject biting one level down. Patching the fence regex
through a shell heredoc into Python wrote a literal `\x02` into the source — the `\2` backreference was
eaten by an escaping layer and reinterpreted as an octal escape. It compiled, and the regex silently
became "two or more markers" with no backreference. Caught only by dumping `repr()` of the line rather
than reading it. Same lesson as the ticket: **check the bytes, don't reason about the escaping.** The
final edit was made with a tool that does literal replacement, no shell layer.

Re-verified after all of the above: the restored `RELEASING.md` snippet still runs verbatim —
`runId=32645968281  jobs parsed=4`, crafted message emitted. Guard: 17 tests green; a scan confirmed no
stray control characters anywhere in the test file.

### Deliberately not done

- The `verify-release-artifacts` unconditional success message noted in Notes above is left for
  CPE-1901, as this ticket directs.
- No attempt to make the snippets work identically in bash and PowerShell. They target PowerShell,
  they say so via their fence tag, and the new guard enforces that every `gh` block declares a shell.
