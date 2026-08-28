---
id: CPE-1936
title: `shellScriptLines` mis-parses two heredoc forms, and the publish path's expected run title is bound to nothing
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Four fix-forward findings from PR #1039's round-3 review. All are latent today — no workflow in the
tree contains the shapes — and all were found by **feeding the parser adversarial input**, not by
reading it. The first two are in `src/lib/shellScriptLines.ts`, which now backs **two** guards
(`channelPurityCoverage.test.ts` and `releaseHangHardening.test.ts`), so its blind spots are shared.

**N8 — a heredoc token inside a quoted string swallows the rest of the step. This one has an unsafe
direction.**

    echo "use <<EOF to start a heredoc"
    cargo run … --bin verify-release-artifacts -- --expect-channel sidecar
    echo tail

    lines    -> ["echo \"use <<EOF to start a heredoc\""]     <- two real lines vanished
    channels -> []

For the channel ratchet this is **safe** — it produces a loud red. For `releaseHangHardening.test.ts`'s
*"no `apt`/`curl` invocation left unhardened"* scan it is the **unsafe** direction: a genuinely
unhardened command silently drops out of the scan. That file's count assertions are a partial
backstop. Fix: match `HEREDOC_START` against the out-of-quote skeleton only — the scanner already
tracks quote state.

**N7 — an indented terminator closes a plain `<<EOF` early.** The check is `raw.trim() ===
heredocDelim`, but real bash requires column 0 for `<<`; only `<<-` strips leading tabs. So heredoc
body lines get scanned as live code:

    lines    -> ["cat <<EOF", "cargo run --bin verify-release-artifacts -- --expect-channel sidecar", "EOF"]
    channels -> ["sidecar"]        <- pulled out of a heredoc BODY

`HEREDOC_START` already captures whether it saw the `-`; it simply is not carried through to the
terminator comparison.

**N9 — an unterminated quote leaves a trailing comment unstripped.** `echo "oops # not stripped` comes
back unchanged. Neutralised in practice by the per-line anchored matching added in round 3, and an
unterminated quote is a shell syntax error in its own right. Lowest priority of the four.

**N10 — the publish path's expected run title is bound to nothing.** The literal
`"Release (sidecar) "` now lives in **three** places with no test tying them together:
`.claude/commands/run.md`'s exact-match `select`, `RELEASING.md`, and `release-sidecar.yml:34`'s
`run-name:`. Editing `run-name:` silently breaks the publish-path lookup. It **fails closed** (the
lookup throws rather than publishing an unverified draft), so this is a maintenance hazard rather
than a safety hole — but it is the **provenance-claim shape** CPE-1933 is filed about, and a one-line
assertion tying `run.md`'s expected title to the workflow's real `run-name:` is exactly this repo's
house style.

## Explicitly NOT in scope

The same review found four **false REDs** in `isRealInvocationLine()` — `cd crates && cargo run …`,
`bash -c "cargo run …"`, `$VERIFY --expect-channel sidecar`, and the `--bin=verify-release-artifacts`
equals-form. Each would make the ratchet cry unguarded on a legitimate refactor. They are **loud, not
silent**, which is the correct failure direction, and widening the predicate to accept them risks
re-opening the decoy family that round 3 closed. Leave them unless one actually bites.

## Acceptance criteria

- [ ] Fix N8 by matching `HEREDOC_START` against the out-of-quote skeleton. **Verify against
      `releaseHangHardening.test.ts` specifically**, since that is the guard where this direction is
      dangerous.
- [ ] Fix N7 by carrying the captured `-` through to the terminator comparison.
- [ ] Fix or explicitly document N9.
- [ ] Bind the run title (N10) with a test that reads `run-name:` out of `release-sidecar.yml` and
      asserts `run.md` expects exactly that — a *derivation*, not a restated constant (CPE-1933).
- [ ] **Red-proof each by feeding the parser the adversarial input above**, not by reading it. Every
      one of these was found that way and none would have been found by inspection.
- [ ] While in there: the scanner is hand-rolled and character-by-character, which is the shape that
      has produced three separate bugs in this repo tonight. Consider whether the adversarial cases
      deserve to be a permanent fixture table rather than one-off checks.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1039's round-3 review, which recommended all four as
fix-forward. **N6 from that same list — `|| true` neutering a real invocation while the ratchet
reports full coverage — was NOT deferred**; it is a false green in a coverage guard and was fixed in
#1039 itself.

Related: **CPE-1908** (the guard), **CPE-1929** (guards that cannot go red), **CPE-1933** (provenance
claims bound to nothing).

## Two shapes measured 2026-08-27 by PR #1060 — one CLOSED, one still open

PR #1060 (CPE-1933) built a Rust port of `shellScriptLines.ts` and made both languages run against a
**shared** `src/lib/shellScriptLines.cases.json`. The oracle immediately caught a divergence, and the
port turned out to be the safer half.

**CLOSED in PR #1060 — the here-string phantom heredoc.** `HEREDOC_START`'s `(?!<)` only refuses a
match beginning at the **first** `<` of `<<<`; the engine retries from the second, the lookahead sees
a space, and `` closes on the word:

    done <<< "names"
    echo after

    TS   -> ["done <<< \"names\""]                    <- swallows the rest of the script
    Rust -> ["done <<< \"names\"", "echo after"]       <- correct

A false negative in exactly the direction this module's own header calls unsafe for a coverage
ratchet — an unhardened command silently drops out of the scan. Fixed by requiring **both** guards,
`(?<!<)<<(?!<)`, pinned with a shared-line case (`echo a <<<"lit" && cat <<EOF` must still open `EOF`)
so the fix cannot later be "corrected" into over-refusing. The Reviewer attacked it with eight
over-refusal probes — all clean — and found the fix closes a **wider** class than the case that
exposed it: the old regex also mis-fired on an **unquoted** here-string word (`done <<< names`),
where `` matches empty.

**STILL OPEN and belongs here — `<<` inside a quoted string opens a phantom heredoc.** Measured:

    echo "a << EOF"
    cargo run --bin x -- --expect-channel sidecar     <- swallowed

Same unsafe false-negative direction. It fails **identically under the old and new regex**, so it is
pre-existing, and **both languages share it**, so the shared oracle agrees rather than flagging it —
which is worth knowing about oracles generally: a shared case file catches divergence, not shared
blindness.

Note this is closely related to **N8** already in this ticket (a heredoc token inside a quoted string
swallowing the rest of the step). Treat them together: the fix for both is to match `HEREDOC_START`
against the **out-of-quote skeleton**, which the scanner already tracks.

**When fixing, add the case to `src/lib/shellScriptLines.cases.json`, not only to the TS test** — the
Rust port reads it at run time, and a fix landed on one side only would put the two implementations
back into the disagreement PR #1060 just resolved.

## Work Log — 2026-08-27

Worked as a sprint Worker. All four findings closed; both implementations changed in lockstep and
pinned by the shared oracle.

### Reproduced first, as failing cases in the shared oracle

Both cases were added to `src/lib/shellScriptLines.cases.json` **before** either parser was touched,
and both languages went red on them — which also settles that the Rust port really does read that
file at run time rather than claiming to.

**N8 — a heredoc token inside a quoted string.**

    echo "use <<EOF to start a heredoc"
    cargo run --bin verify-release-artifacts -- --expect-channel sidecar
    echo tail

    wrong -> ["echo \"use <<EOF to start a heredoc\""]           (2 real lines gone)
    right -> all three lines

**N7 — an indented terminator closes a plain `<<EOF` early.**

    cat <<EOF
      EOF
    cargo run --bin verify-release-artifacts -- --expect-channel sidecar
    EOF
    echo after

    wrong -> ["cat <<EOF", "cargo run … --expect-channel sidecar", "EOF", "echo after"]
    right -> ["cat <<EOF", "echo after"]

### N8 is LIVE in this tree, not latent

`ffmpeg-pin-freshness.yml` writes GitHub multi-line outputs as
`echo "failures<<PINFAIL_EOF" >> "$GITHUB_OUTPUT"` — precisely the N8 shape — in three places.
Measured old vs new over the real files:

| scan | before | after | delta |
|------|--------|-------|-------|
| whole-file (`workflow_scan`'s consumers) `ffmpeg-pin-freshness.yml` | 142 logical lines | 302 | **+160** |
| per-step (`step.run`, the TS guards' view) `ffmpeg-pin-freshness.yml` | 198 | 222 | **+24** |

The 24 per-step lines were spread over two `check-pins` steps and included an `exit 1` and the whole
error/warning branch. No other workflow changed by a single line, so no live guard was answering
wrongly today — but the blind window in that file was 24 lines wide (160 whole-file), and any `curl`
added after one of those `echo` lines would simply have dropped out of `releaseHangHardening`'s
"nothing left unhardened" scan.

### Fixes

- **N8** — `HEREDOC_START` (a regex, which cannot be told about quote state) is replaced by
  `heredocOpener()`, a scanner that walks the line with the *same* quote/escape rules
  `stripShellComment` uses and only considers a `<<` found outside quotes. Rust gains the same quote
  tracking in `heredoc_opener()`.
- **N7** — the captured `-` is now carried through in `HeredocOpener.dashed`, and `closesHeredoc()`
  requires the terminator to be the delimiter alone, indented **no more than the opener line**.
  Relative rather than column-0 on purpose: `release_workflow_wiring.rs` runs this over whole `.yml`
  FILES, where `release-sidecar.yml`'s `cat > "$notes_file" <<'EOF'` and its `EOF` both sit ten
  spaces in — a column-0 rule would leave that heredoc open for the rest of the file and empty the
  scan. For a genuine shell script (opener at column 0) the rule is bash's exactly.
- **N9** — documented at the site in both files and pinned as a `KNOWN GAP` case, rather than fixed.
  The obvious fix ("an unterminated quote was never a quote") would truncate the first line of a
  legal multi-line quoted string, which is the unsafe direction; the real fix is cross-line quote
  state.
- **N10** — new `src/lib/publishRunTitleBinding.test.ts` derives the expected title instead of
  restating it: the prefix is parsed out of `release-sidecar.yml`'s `run-name:` (as YAML, so a
  comment mentioning the key cannot be read as the key), and the `<TAG>` placeholder is read out of
  `run.md`'s own plain-channel `headBranch -ceq "…"` line. It also asserts the tag interpolation is
  at the END of `run-name` (otherwise `"<prefix><TAG>"` is the wrong shape) and that `RELEASING.md`
  quotes the real `run-name:` line.

### Red-proofs (each fix sabotaged alone)

| sabotage | red | green |
|----------|-----|-------|
| `closesHeredoc` returns `true` (the pre-fix `raw.trim() === delim` rule) | 1 case: N7 | 40 |
| `heredocOpener` never enters quote state (`if (false && …)`) | 4 cases: all three N8 shapes + the "quoted `<<TOKEN` must not stop a real heredoc later on the line" case | 37 |
| `release-sidecar.yml`'s `run-name:` → `"Release (sidecar build) …"` | 2 of the 3 N10 assertions, naming old and new values | 1 |

Neither heredoc sabotage touched the other's cases, so the two fixes are independently load-bearing.

### Neighbouring escape forms checked

Thirteen further cases were added to the shared file, all green on the fix: `<<` in a
**single**-quoted string; the live GitHub-output idiom; a quoted `<<TOKEN` followed by a real `<<EOF`
on the same line; a **backslash-escaped** quote leaving the `<<` genuinely unquoted (bash agrees it
opens a body); `<<-` with an indented terminator; a uniformly indented (YAML-embedded) heredoc; a
terminator carrying a trailing comment (body, not a terminator); the delimiter as a substring of a
body line; `<<"EOF"`; mismatched delimiter quotes; a trailing comment after the opener; an
**unquoted** here-string word (`done <<< names`, the wider class CPE-1933's `(?<!<)…(?!<)` pair
closed).

Three shapes are left open and now say so at the site: `$(( a << b ))` reads as a heredoc named `b`
(suppressing it needs `$(( … ))` depth tracking a plain `( (cmd) )` would false-trigger, trading a
false negative for a false positive); two heredocs on one line (`cat <<A <<B`) tracks only `A`
(unchanged from before); and a partially quoted delimiter (`<<E"OF"`) reads as `E`.

**Trailing comments**: confirmed not regressed. `cat <<EOF   # writes the notes` still opens `EOF`
(the comment is stripped before the scan), the `-ceq` extractor in the new N10 test strips PowerShell
comments before matching, and `a_trailing_comment_cannot_smuggle_a_flag_back_into_a_scan` is
untouched and green.

**One honesty correction while in there.** The N10 test's comment-stripping is *not* reached by any
string in `run.md` today — none of its comments spells the full `$_.displayTitle -ceq "…"` shape — so
asserting it in a comment would have been CPE-1929's "safe and unverifiable at once" pair. It is kept
(one edit away from mattering) and given real coverage against a synthetic document, with the
un-stripped reading measured alongside as a literal red-then-green; the header says which half of the
anchoring is load-bearing today.

### Housekeeping

`fencedBlocks`/`POWERSHELL_LANGS` moved out of `runbookJqQuoting.test.ts` into a new
`src/lib/markdownFences.ts` so the new guard reuses the reviewed fence parser instead of growing a
second one (CPE-1950's "remove the duplication where you can"); the old names are re-exported so that
file's own property tests stay where they were reviewed.

### Verification

- `npm run check` — 0 errors, 0 warnings.
- `npm test` — **5006 passed / 2 skipped, 349 files** (from 4985 / 348: +15 shared cases in
  `shellScriptLines.test.ts`, +6 in the new binding test).
- `crates/updater-verify`: `cargo clippy --all-targets -- -D warnings` and the same with
  `--all-features` both clean; `cargo test` **147 passed** (from 145: +2 new `workflow_scan` unit
  tests), sources touched first so the run is not a stale-cargo false green.

## Work Log — 2026-08-27 (round 2 — Reviewer corrections, comment-only)

The Reviewer reproduced every substantive claim of round 1: both headline deltas line for line, the
"no other workflow moved" sweep over all eight workflows *plus* the three
`.github/workflows/scripts/*.sh`, the "no file LOST a line anywhere" direction, both sabotages
independently, and the run-time-read proof (it edited one `expected` in the shared oracle and watched
`cargo test the_port_matches` panic). Three **comment** defects came back. No code changed in this
round; the diff is four files, comments only.

### 1 (blocker) — the honesty paragraph named the wrong half, and there is no right half

`publishRunTitleBinding.test.ts`'s header said the FENCE filter was what excludes today's prose and
that the `#`-comment stripping was the unreached half. Re-measured here, each sabotage scoped to the
production read (`powershellLines`) so the synthetic documents below keep both filters:

| sabotage against the real `run.md` | result |
|---|---|
| drop the fence filter (whole `.md` scanned, comments still stripped) | **6 passed / 6** |
| drop the fence filter **and** the comment stripping | **6 passed / 6** |

**Neither** filter is reached. `grep -n -- "-ceq" .claude/commands/run.md` gives the reason: only
lines **83** and **92** carry the `$_.displayTitle` / `$_.headBranch -ceq "…"` shape and both are
live code. Line 78 is a `#` comment that discusses `-ceq` without spelling the comparison; lines 105
and 119 use a different property (`$_.name`), the latter compared against a variable rather than a
literal. The prose the header cites is not in that shape either — neither the in-fence comment
quoting `displayTitle "Release (sidecar) v1.2.3-sidecar-decoy"` nor the out-of-fence paragraph
quoting `"Release (sidecar) <TAG>"`.

Nothing here is untestable, which is the difference from CPE-1929's unreachable-guard pair: both
filters carry real synthetic coverage (the RED/GREEN decoy pair; "prose outside every fence is not
scanned at all"), and both are one edit away from mattering, so both stay. The paragraph now says
exactly that, with **both measured numbers at the site**.

**This supersedes the "One honesty correction while in there" paragraph in round 1's log above, which
carried the same wrong claim.**

### 2 — "(as bash does)" is only half true

The shared case named the backslash-escaped opener as agreeing with bash. Bash *does* open a body for
`echo \"<<EOF\"`, but it takes the delimiter as **`EOF"`**, not `EOF`:

    $ bash -n t1.sh
    t1.sh: line 4: warning: here-document at line 1 delimited by end-of-file (wanted `EOF"')

and a literal `EOF"` line does close it (verified by *executing* the variant, which printed the body
line as data and then `after`). This parser closes on bare `EOF` and resumes scanning — the
**false-POSITIVE** (body-as-code) direction, the same family as the already-documented `<<E"OF"` gap.
Unreachable in this tree. The case is renamed to a **KNOWN GAP** and folded into that bullet in
`shellScriptLines.ts`; the Rust port's comment already defers to it.

### 3 — two parser comments were inaccurate (both unreachable, so documented rather than fixed)

- **`closesHeredoc`**: the shape it accepts that bash would not is a terminator indented **≤** an
  already-indented opener, not "less than" — the equal case is the easy one to misstate. Measured:
  `cat <<EOF` at column 2 inside `if true; then` with its `EOF` also at column 2 is **body** to bash
  (`bash -n` → "here-document at line 2 delimited by end-of-file (wanted `EOF')", then
  "syntax error: unexpected end of file from `if' command on line 1"), while this closes it.
- **`HeredocOpener.dashed`**: said bash strips leading TABS; the code accepts **any** indent.
  Measured: for `<<-END`, a **space**-indented `END` stays body, a tab-indented one closes. Corrected
  in the TS doc and in the Rust twin (`workflow_scan.rs`) in lockstep.

### Unreachability, enumerated rather than recalled (CPE-1932)

`git grep -- '<<' .github/workflows` → nine lines: three real openers (`ci.yml:367`, `ci.yml:386`,
`release-sidecar.yml:71`, all `<<'EOF'` at column ≥ 10), the three `echo "…<<…_EOF"` N8 shapes in
`ffmpeg-pin-freshness.yml`, and three `<<<` here-strings. `git grep -- '<<' -- '*.sh'` → **none at
all**, so the three `.github/workflows/scripts/*.sh` contain no heredoc either. No arithmetic `<<`,
no two heredocs on one line, no partially quoted delimiter, **no `<<-` anywhere**. Recorded in
`heredocOpener`'s comment so the next reader gets a measurement rather than a recollection.

Separately, the indentation judgement call from round 1 stands and is necessary:
`release-sidecar.yml:71`'s opener and its `EOF` both sit at column 10 (`cat -A` confirmed), so a
column-0 rule would leave that heredoc open for ~294 lines and empty the scan.

### Live impact, stated the stronger way

`releaseHangHardening.test.ts`'s header records that **CPE-1849 folded `ffmpeg-pin-freshness.yml`
into `GUARDED`** — so before this fix the hardening scan really *was* blind to 24 per-step logical
lines (31 → 39 and 35 → 51 across the two `check-pins` steps; 142 → 302 whole-file) of a file it
believed it covered, and no other workflow moved by a line. Nothing answered *wrongly* only because
that blind window contained no `curl`, no `apt`, no `--expect-channel` and no `--locked` — established
by **reading** the newly visible lines, not inferred from guards staying green. The module comment
now says it that way instead of "NOT latent".

### Out of scope, filed as CPE-1969

Two further CPE-1932 gaps the Reviewer found and did not ask for here:
`lockfileLockedGuard.test.ts`'s `WORKFLOW_FILES` is a hard-coded 5-file list, and no consumer scans
`.github/workflows/scripts/*.sh` at all.

### Verification (round 2)

- `npm run check` — 0 errors, 0 warnings.
- `npm test` — **5024 passed / 2 skipped, 350 files**, against a `b5658d93` baseline of **5003 / 2,
  349** → **+21 tests, +1 file**. Round 1's log quoted absolutes taken before a main merge; the delta
  was and is exact, which is precisely why the delta is the number that gets reported.
- `crates/updater-verify`: `cargo clippy --all-targets -- -D warnings` clean with sources touched
  first; `cargo test` **147 passed** (79 + 31 + 21 + 13 + 2 + 1 + 0 + 0). The crate has **no
  `[features]` section**, so `--all-features` is identical to the default build and its instant second
  run is a legitimate cache hit, not a stale green.
- Every sabotage reverted; the working tree carries only the intended comment diff.
