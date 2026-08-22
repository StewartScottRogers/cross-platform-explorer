---
id: CPE-1849
title: ffmpeg-pin-freshness pairs --retry with --max-time and has no --retry-max-time
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-21
closed:
---

## Problem

`.github/workflows/ffmpeg-pin-freshness.yml:251` and `:409` both pair `--retry 3` with `--max-time 30`
and no `--retry-max-time`.

CPE-1824 established, by measurement rather than by reading, that this combination does not bound what
it looks like it bounds: `--max-time` is **per attempt** and its counter **resets on every retry**, and
`--retry-all-errors` makes a `--max-time` expiry *itself* a retryable error — so the timeout meant to
stop a stall is what triggers the next attempt. Measured against a deliberately stalling server, the
`ci.yml` sites ran **1101 seconds** against a claimed 180-second bound, printing six separate
`Operation timed out` messages. Adding `--retry-max-time` brought the same case to **182 seconds**.

## Why this is NOT a repeat of CPE-1824 — and why it is Medium, not Low

**Raised from Low to Medium on 2026-08-22, and this section rewritten rather than appended to.** It
argued a position the work then falsified, and leaving it standing would make this ticket the exact
once-true-now-false artefact the CPE-1824 family exists to remove — the same reason CPE-1824's stale
exclusion note was deleted rather than amended.

What the original section got right, and what still stands: the defect CPE-1824 fixed was a **false
claim recorded as verified fact**, and this file's `--max-time` comment was already **accurate**
("--max-time bounds each attempt"). Nothing here was lying. That remains true and is why this is a
different ticket rather than a repeat.

What it got wrong was the *severity*, on two counts:

1. **"The worst case is also much smaller."** Smaller per call, yes — but the original reasoning
   stopped at one call. The `HEAD-check pinned assets` step invokes one shared `head_check()` helper
   **five times under a single `timeout-minutes: 5`**, so the number that matters is `5 x 126.7s =
   ~634s` against a 300s cap, not `3 x 30s`. The multiplier was never in the analysis.

2. **"What remains is that the workflow has no explicit series bound."** That framing makes it
   sound like tidying — a true statement missing a formality. It is not. Measured end-to-end on the
   step's own script (Work Log below): the step was **SIGKILLed at 300.9s having checked 2 of its 5
   assets**, emitting **no `::error::` annotation** and writing **nothing** to `$GITHUB_OUTPUT`.

The second point is the reason for Medium. This is the **only** workflow that detects a rotted
native-dep pin, and it was silently losing 40% of its coverage while producing a failure
indistinguishable from an infrastructure flake. A pin could rot in `pdfium macos` — the fifth and
never-reached check — and this workflow would never say so. Silent partial coverage in the detector
of last resort is not Low.

## Acceptance criteria

- [x] Both sites either gain a `--retry-max-time` coherent with the step's `timeout-minutes`, or record
      why the outer backstop is sufficient here. CPE-1824's arithmetic is the precedent: curl checks the
      retry timer *before* starting each retry and lets an in-flight attempt finish, so worst case is
      `retry-max-time + max-time` <!-- SUPERSEDED: the real bound is retry-max-time + retry-delay +
      max-time; this criterion's formula was inherited from CPE-1824 and is wrong. Disproved by
      measurement in the Work Log's "CORRECTION (round 2)". The criterion is still met, against the
      corrected formula. -->, and the value must satisfy `N + max-time < timeout-minutes` with margin
      — the point being that curl loses on its own terms, with a real exit code and `--fail`
      diagnostics, rather than being killed opaquely by the runner.
- [x] Confirm what outer `timeout-minutes` actually applies to each site before choosing a value. Do not
      assume one exists.
- [x] CPE-1824's guard (`src/lib/releaseHangHardening.test.ts`) scans every non-comment curl line in the
      three workflows it covers. Decide whether to extend its scan to this file, and say why either way.
      If extended, the exclusion note currently recording this file as deliberately out of scope must go.
- [x] Re-run a positive control after any change: a real download through the modified flags, reporting
      exit code, elapsed time and byte count — not just that the workflow parses.

## Notes

Found by the CPE-1824 round-2 worker, which deliberately did **not** absorb it: the defect that ticket
existed to fix is not present here, and this file has several branches in flight on it. The exclusion is
documented in that ticket's test file so it reads as considered rather than missed.

Read CPE-1824's Work Log first — it carries the stall-server harness and the measured before/after, so
this ticket does not need to re-derive the mechanism.

## Work Log

**2026-08-22** — Both sites bounded, guard extended, and the failure reproduced end-to-end on the
workflow's own script rather than on a hand-built approximation of it.

> **Round 2 (same day) — Reviewer returned CHANGES REQUESTED and was right on every count.** It
> independently reproduced the headline measurement (301,951 ms / exit 137 / 2 of 5 assets before;
> 155,647 ms / exit 1 / 5 of 5 after — within 1–2% of the figures below) and confirmed the coverage
> framing, then found that this PR, whose subject is *a wrong reason left standing because its
> conclusion looks right*, had introduced three new wrong reasons of its own:
> a **fabricated `#`** in `%{http_code}` propping up a quote-awareness claim; a **worst-case formula
> missing its `--retry-delay` term**, disproved by measurement; and a **value assertion that matched
> a prefix**, so `--retry-max-time 200` passed. It also showed the arithmetic gap round 1 admitted
> and declined to fix was live on *both* terms of the product, and that the ticket's own "Why this is
> Low" section had been falsified by this very Work Log and left standing.
>
> All four fixed, plus the recommended arithmetic assertion; the section rewritten and the priority
> raised to Medium. Each correction is marked **CORRECTION (round 2)** in place below rather than
> appended at the end, so nobody reads the superseded claim first. Round 1's conclusions all survive
> — every correction either strengthened the argument or closed a hole in the evidence for it, and
> none changed the shipped flags.

### The outer cap, established before choosing anything

Both curl sites live in the single `check-pins` job of `ffmpeg-pin-freshness.yml`, and both of their
steps do carry their own cap — this was checked, not assumed:

| site | step | outer cap | curl calls under it |
|---|---|---|---|
| the `head_check()` helper | `HEAD-check pinned assets` | `timeout-minutes: 5` (300s) | **five** invocations of one shared helper |
| the candidate-URL check | `Validate the recommendation before publishing it` | `timeout-minutes: 5` (300s) | **two**, via a two-element `for entry in` loop |

Job-level cap is `timeout-minutes: 30`, but it is not the binding constraint for either site: it is
documented in the file as the sum of the five per-step caps plus slack, so a step cap always fires
first. The relevant number for the arithmetic is therefore 300s at both sites, not 1800s.

Note the call counts differ in *shape*: the first step has one curl line called five times, the
second has one curl line inside a two-iteration loop. Both multiply the worst case; neither is
visible from the curl line itself. That is why the multiplier got its own guard assertions (below).

### The arithmetic, re-derived and then measured

The ticket handed me `4 x 30s + 3 x 2s = 126s` per `head_check`. Re-derived independently
(`--retry 3` is three retries *after* the initial attempt = four attempts; `--retry-delay 2` is a
fixed 2s slept three times), then **measured** against a stall server rather than trusted:

    curl -sSL --max-time 30 --retry 3 --retry-delay 2 --retry-connrefused \
      -o /dev/null -w '%{http_code}' --head http://127.0.0.1:8099/a
    → exit 28, elapsed 126,728 ms, FOUR "Operation timed out after 3000x ms" messages

126.7s, matching the derivation to within 0.6%. Four messages is the mechanism visible: one full
30s clock per attempt, not one shared 30s budget. So five calls is ~634s against a 300s cap — the
step blows it during the **third** call, exactly as the ticket predicted.

The stall server is CPE-1824's harness rebuilt: accept, read the request, then send nothing, ever.
That is the one failure mode `--retry` cannot see, because the transfer never reaches a terminal
state on its own.

### The "in-flight attempt finishes" behaviour, verified rather than inherited

The whole worst-case formula rests on it, so it was checked two independent ways.

**From curl's own docs** (`docs/cmdline-opts/retry-max-time.md`, fetched from curl's repo, not
recalled): *"Before each new retry is started, curl checks whether the elapsed time has reached the
specified limit. If it has, no further retries are performed. A transfer that has already started is
allowed to run to completion even if this makes the total wall clock time exceed the limit."*

**By experiment**, scaled down so the overshoot is unambiguous — `--max-time 6 --retry 5
--retry-max-time 10 --retry-delay 1` against the stall server:

    → exit 28, elapsed 13,755 ms, TWO timeout messages

Attempt 1 died at 6s; elapsed 6 < 10 so a retry was permitted at ~7s; that attempt then ran its
**full** 6s and ended at ~13s — past the 10s `--retry-max-time`. Before a third retry, elapsed 13
>= 10, so it stopped.

### CORRECTION (round 2): the formula also needs the `--retry-delay` term

The Reviewer disproved `retry-max-time + max-time` by experiment, and the disproof was reproduced
here independently before acting on it. curl checks the retry timer, **then sleeps `--retry-delay`,
then** runs the attempt it just permitted — so the supremum is
`retry-max-time + retry-delay + max-time`. Measured here on curl 8.21.0 against the stall server:

| flags | elapsed | `rmt + mt` predicts | verdict |
|---|---|---|---|
| `--max-time 2 --retry 10 --retry-delay 3 --retry-max-time 4` | **7,928 ms** | 6,000 ms | **EXCEEDED** |
| `--max-time 2 --retry 10 --retry-delay 2 --retry-max-time 4` | **7,039 ms** | 6,000 ms | **EXCEEDED** |
| `--max-time 2 --retry 10 --retry-delay 1 --retry-max-time 4` | 5,581 ms | 6,000 ms | inside |
| `--max-time 6 --retry 5 --retry-delay 1 --retry-max-time 10` | 14,379 ms | 16,000 ms | inside |

The bottom two rows are why this survived three rounds of review across two tickets: **a 1s delay is
too small a term to push the total past the wrong bound**, and both CPE-1824's scaled experiment and
this ticket's round-1 one happened to use `--retry-delay 1`. A confirming experiment chosen from
inside the wrong model confirms the wrong model. The corrected formula is now *computed* by the
guard rather than asserted in prose — see the arithmetic assertion below.

CPE-1824's own sites carry the same omission: it recorded `150 + 180 = 330s` for the pdfium sites,
where the corrected figure is `150 + 3 + 180 = 333s`. Its conclusion survives too (333 < 360).

### Value chosen: `--retry-max-time 20`, at both sites

Sized against the **binding** step, the five-call one, using the corrected formula:

- worst case per call = `retry-max-time + retry-delay + max-time` = `20 + 2 + 30` = **52s**;
- five calls = **260s**, inside the 300s cap with **40s (13%) of margin**;
- the two-call site is the same 52s worst case x 2 = **104s**, 196s of margin.

Why the same value at both rather than a looser one where there is room: a larger `N` buys the
retries nothing. Their actual job here is connection-level failure (refused/reset), which fails in
milliseconds, so three retries plus their 2s delays finish inside ~6s — far under 20. A larger `N`
would only widen the stall worst case. One number, one rule, tighter site governs.

**Why the value is necessarily *below* `--max-time`, which is worth stating because it looks odd.**
It falls straight out of the multiplier, not out of any property of `--max-time`: worst case per
call is `N + 2 + 30`, so `N = 30` puts five calls at **310s — already OVER the 300s cap**, and the
largest `N` that fits at all is 27. (Round 1 said "exactly 300s, the cap with zero margin"; that was
the missing-`retry-delay` formula. The correction **strengthens** this argument rather than
weakening it.) Five calls under one cap is what forces `N` well under 30 here; the two-call site
would tolerate up to `N = 118`, which is precisely why the value must not be copied between sites
without recounting the calls.

A consequence worth naming, since it is what makes 20 look wrong at a glance: because `20 < 30`, a
**fully stalled** first attempt (which burns the entire 30s) has already passed the retry limit, so
no retry is attempted at all in that case. That is the correct trade rather than a regression —
retrying a stall is exactly the self-feeding loop CPE-1824 measured — and it costs nothing in
reporting, because the step classifies the resulting exit-28/`000` as **inconclusive** and fails
loudly asking for a re-run. Fast-failing errors, which are what the retries are actually for, are
untouched: they finish in ~6s, far inside 20.

The margin is the point, per CPE-1824's precedent: curl loses on its **own** terms — a real exit
code and an `http_code` of `000` that the step's `*)` branch turns into a named `::error::` line —
instead of being killed opaquely by the runner.

### The end-to-end proof: the workflow's real script, before vs after

Better than testing a hand-copied flag string: the `run:` blocks were extracted from the parsed
workflow with PyYAML and executed as-is, with the five asset URLs pointed at the stall server, under
`timeout --signal=KILL 300` to simulate the runner's `timeout-minutes: 5`. The "before" script is
the same extracted script with `--retry-max-time 20 ` removed.

| | elapsed | exit | assets reported | `::error::` emitted | `$GITHUB_OUTPUT` |
|---|---|---|---|---|---|
| **before** | **300,967 ms** | **137 (SIGKILL)** | **2 of 5** | **none** | **0 bytes** |
| **after** | **152,591 ms** | **1** (the step's own deliberate loud failure) | **5 of 5** | full inconclusive list, all five named | 0 bytes (correct — the inconclusive path exits 1 before writing `stale=`) |

The before run died partway through the third `head_check`, having printed eight timeout messages
and two asset lines — the predicted third-call kill, observed. Nothing downstream could tell that
run apart from an infrastructure flake: no annotation, no verdict, no output.

The after run is 152.6s against a predicted 5 x 31.1 = 155.5s (within 2%), and — this is the part
that matters more than the number — **all five assets get checked and named**. The step is written
`set -uo pipefail`, deliberately *not* `-e`, with the comment "every asset must be checked even if an
earlier one is bad". The unbounded retry series was silently defeating that design intent: assets 4
and 5 were never reached. So this was not only a slowness bug, it was a **coverage** bug in the
workflow's own stated contract.

### Positive control — real network, real bytes

All five real pinned asset URLs (`chromium/7961`, `autobuild-2026-07-31-14-10` /
`n8.1.2-34-g9b6c8969e0`, derived the same way the workflow derives them) through the **exact new
flag set**:

    HTTP 200 exit=0  1002ms  ffmpeg-n8.1.2-34-g9b6c8969e0-win64-lgpl-8.1.zip
    HTTP 200 exit=0  1323ms  ffmpeg-n8.1.2-34-g9b6c8969e0-linux64-lgpl-8.1.tar.xz
    HTTP 200 exit=0  1959ms  pdfium-win-x64.tgz
    HTTP 200 exit=0  1730ms  pdfium-linux-x64.tgz
    HTTP 200 exit=0  2820ms  pdfium-mac-arm64.tgz
    step total 16,989 ms against the 300,000 ms cap

And a real **body** download through the same flags (`--head` dropped), since a HEAD alone proves
nothing about a transfer: `pdfium-linux-x64.tgz` → **exit 0, HTTP 200, 1,641 ms, 3,650,783 bytes**,
gzip magic `1f8b` (the byte prefix `release-sidecar.yml` itself checks), valid tar, 49 members,
`lib/libpdfium.so` present — the member the Linux staging step actually extracts.

### Guard decision: EXTEND, and the exclusion note is deleted

`ffmpeg-pin-freshness.yml` is added to `releaseHangHardening.test.ts`'s `GUARDED` list. The reason
CPE-1824 gave for excluding it was scope plus "adding it before fixing the sites would just turn the
guard red" — both of which are spent the moment the sites are fixed. Leaving the note would make it
the next once-true-now-false claim in a file whose whole subject is once-true-now-false claims, so
the note is removed rather than amended.

**The continuation-joining was checked here, not assumed to generalise.** Adding the file to
`GUARDED` *before* fixing it was run deliberately as the pre-fix check, and reported **both** sites:

    ffmpeg-pin-freshness.yml: every curl combining --retry with --max-time also carries --retry-max-time
    → expected [ …(2) ] to deeply equal []
      "check-pins / HEAD-check pinned assets: code=$(curl -sSL --max-time 30 --retry 3 …"
      "check-pins / Validate the recommendation before publishing it: code=$(curl -sSL …"

**Two** things fall out of that one run: `logicalLines()` really does join this file's backslash
continuations (each offender is reported as one joined logical line carrying both flags), and the
repo's bounded-subset `parseYaml` really does parse this file (`parseWorkflow` throws otherwise, and
did not).

**CORRECTION (round 2): round 1 claimed a third thing, and it was false.** It said the same run
confirmed `stripShellComment()`'s quote-awareness "because the joined tail contains `-w
'%{http_code}'` whose `#` is quoted". **There is no `#` in `%{http_code}`** — the characters are
`% { h t t p _ c o d e }` — and neither curl invocation in the file contains a `#` anywhere. Worse,
the probe could not have proven it even if one had: `--retry` and `--max-time` both sit *before* the
`-w` tail, so a strip that truncated there would still leave an offender to report and the run would
have looked identical. Confirmed by mutation rather than by argument: replacing
`stripShellComment()`'s whole body with a naive `line.slice(0, line.indexOf("#"))`, no quote
tracking at all, left **all 21 tests passing**. Nothing in the suite exercised the property.

On a ticket whose subject is a wrong reason left standing because its conclusion looked right, that
is exactly the defect being fixed, reintroduced one level down — the second time this family has
produced that shape (CPE-1824 round 2 did it with `--retry-all-errors`). Two responses, not one:
the claim is deleted, **and** the gap it papered over is closed. A new describe block,
`logicalLines() handles shell comments and continuations`, exercises the property directly on
synthetic input — a quoted `#` in a URL fragment must not truncate the line (the **silent** failure
direction: the curl vanishes and the pairing scan reports a clean pass on an unbounded site), a `#`
mid-word must not open a comment, a real trailing comment must still be stripped, and a
three-physical-line continuation must still join. Red-proofed against the same naive-strip mutation:
**2 failed / 24 passed**, where before the whole suite was green.

**Two assertions added beyond the pairing scan**, because for this file the sizing rests on inputs a
future edit could move without touching any curl line: `HEAD-check pinned assets` still makes exactly
five `head_check` calls under `timeout-minutes: 5`, and `Validate the recommendation` still loops
over exactly the two-element `for entry in "win64 …" "linux64 …"` list under its own
`timeout-minutes: 5`. Add a sixth asset, or drop a cap to 4 minutes, and `20` silently stops being
correct while every flag still reads fine.

### The arithmetic assertion (round 2) — the gap round 1 admitted is now closed

Round 1 pinned only the **inputs** to the sizing and left the arithmetic to prose, recording the
omission honestly under "not verified" and calling the fix disproportionate for a Low ticket. Both
halves of that judgement were wrong. The Reviewer showed the gap was live on **both** terms of the
product, each on its own sufficient to break the bound with every test green:

- `--max-time` raised 30 → 300: **all tests passed** (worst case per call 322s, five calls 1610s
  against a 300s cap);
- `--retry-max-time 20` → `200`: **all tests passed**, because
  `"--retry-max-time 200".includes("--retry-max-time 20")` is **true** — the spot check pinned a
  *prefix*, not a value. CPE-1824's `toContain("--retry-max-time 150")` has the identical shape.

So the file has now had two tickets about arithmetic nobody was checking, and everything needed to
check it was already parsed. Added:

- `flagValue(flag, n)` → `/(?<![\w-])--flag\s+n(?![\d.])/`, replacing all three `toContain` value
  spot checks (the two CPE-1824 ones included — the pattern was reproduced from there, so fixing
  only my copy would leave the same hole one line up).
- `curlWorstCaseSeconds(line)`, reading the numbers off the joined curl line and applying the
  **corrected** `retry-max-time + retry-delay + max-time` formula (floor of 1 for an absent
  `--retry-delay`, since curl's default backs off from 1s rather than being 0).
- A per-site assertion that `calls x worstCase < timeout-minutes x 60 x 0.9`, failing with the real
  numbers in the message rather than "expected true". The 10% margin is the substantive requirement,
  not rounding slack: it is what keeps curl losing on its own terms.

This assertion would have caught the missing `retry-delay` term by construction, and it is written
against the numbers rather than against the literal `20`, so a future edit may raise the cap, drop a
call, or change `--max-time` freely as long as the product still fits.

**Deliberately NOT extended to `ci.yml`'s three pdfium sites**, and the honest reason is not scope:
run the corrected formula over them and they come to `150 + 3 + 180 = 333s` against a 360s cap —
inside it, so not broken, but only **27s / 7.5% of margin**, under the 10% this block requires.
Folding them in would turn the guard red on work this ticket did not do, and loosening
`MIN_MARGIN_FRACTION` until they passed would convert a finding into a rubber stamp. It is flagged
in the test file for whoever picks it up. **Follow-up worth filing:** whether 27s is adequate margin
for a step that also untars and copies.

Guard went 15 tests → **26**: pairing scan + non-vacuity for the new file, two sizing-input
assertions, two arithmetic assertions, and five `logicalLines()` unit tests.

**Red-proofs — eight in total, each reverted immediately. Rounds 1–4:**

1. Deleted `--retry-max-time 20 ` from the `head_check()` curl → **2 failed / 17 passed** (pairing
   scan `expected [ …(2) ] to deeply equal []`; non-vacuity `expected '…' to contain
   '--retry-max-time 20'`).
2. Added a sixth `head_check "REDPROOF sixth call" …` line → **1 failed / 18 passed**,
   `expected 6 to be 5`. This is the realistic regression — a new asset added without redoing the
   arithmetic — and it is caught.
3. Deleted `--retry-max-time 20 ` from the `Validate the recommendation` curl → **2 failed /
   17 passed**, `expected [ Array(1) ] to deeply equal []`.
4. Changed `Validate the recommendation`'s `timeout-minutes: 5` to `4` → **1 failed / 18 passed**,
   `expected 4 to be 5`.

**Round 2's four, on the new assertions.** Tallies below are against the **final 26-test suite**,
re-run at round-3 head. (Round 2 first recorded 5–7 as `/19` and `/20`, which summed to 21 rather
than 26 — they were measured before the five `logicalLines()` tests were added and never refreshed.
Stale pass counts in a ticket about stale numbers; caught by the Reviewer. The failure counts and
assertion messages were correct throughout, and #8 was already right.)

5. `--retry-max-time 20` → `200`, the exact prefix-match evasion → **2 failed / 24 passed**. The
   arithmetic assertion reported `5 calls x 232s = 1160s against a 300s cap (needs < 270s to keep
   10% margin)`, and the now-bounded value assertion reported `to match /…--retry-max-time\s+20(?![\d…/`.
   Under round 1's suite this mutation was green.
6. `--max-time 30` → `300` → **1 failed / 25 passed**, `5 calls x 322s = 1610s against a 300s cap`.
   Also green under round 1's suite — so both terms of the product are now pinned.
7. `--retry-delay 2` → `30`, confirming the corrected term is load-bearing in the assertion and not
   decoration → **1 failed / 25 passed**, `5 calls x 80s = 400s against a 300s cap`.
8. `stripShellComment()` replaced with the naive `line.slice(0, line.indexOf("#"))` → **2 failed /
   24 passed** (the quoted-fragment test and the mid-word test). The same mutation left round 1's
   entire suite green.

**Round 3's, on the fail-open fix:**

9. Deleted `--retry-delay 2` from the `head_check()` curl → **1 failed / 25 passed**, `need
   --max-time, --retry-max-time AND --retry-delay to compute a worst case; one is missing from:
   code=$(curl …)`. Then, with the flag still absent, `curlWorstCaseSeconds()` was temporarily
   reverted to its `?? 1` default: **26 passed** — the fail-open confirmed by experiment rather than
   argued, since it would have computed `20 + 1 + 30 = 51s`, `5 x 51 = 255s < 270s`, and waved
   through a site whose real bound nobody had checked. Both reverted.

### Round 3 — Reviewer APPROVED round 2; three small items, all closed

It verified all five round-2 findings by **re-running the mutations that exposed them**, not by
reading the diff, and confirmed the `N = 27` ceiling independently (28 gives exactly 300s, which
does not fit strictly).

**1. The corrected figure was not fixed at its source.** Round 2 corrected `330s → 333s` in the
guard's comment but left `.github/workflows/ci.yml` still reading *"curl's real worst case is
150 + 180 = 330s"* and *"leaving ~30s for the step's mkdir/tar/echo"* (really 27s). Not touching
ci.yml was a defensible scope call; leaving a **known-stale arithmetic claim at its source** is the
exact defect class this family exists to remove, and a reader of ci.yml would never have seen the
correction. Both lines now state `150 + 3 + 180 = 333s` and `27s`, with a one-line note on why the
term was missed. Two comment edits, no behaviour change.

**2. `curlWorstCaseSeconds()` failed OPEN on an absent `--retry-delay`.** It defaulted to `?? 1`
"as a floor". curl's default backoff is **exponential**, so the sleep before the final permitted
retry can be 8s or 16s — meaning the substitution **under-states** the worst case, and an
under-stated worst case makes the assertion **pass a genuinely broken site**. That is the one
direction a guard must never fail in, and the round-2 comment was honest that 1 was a floor without
saying which way the error ran. All three flags are now required; an absent one returns null and the
assertion fails loudly with the offending line. Dead code today — every retrying curl across all six
workflows carries an explicit `--retry-delay` — but the shape matters more than whether it fires.
Red-proof 9 above confirms both halves, including that the old default really did wave the site
through.

**3. Stale red-proof tallies**, in a ticket about stale numbers — corrected above, with the reason
recorded rather than quietly overwritten.

**On the exclusion decision, which the Reviewer was asked to second-guess: it agrees, and its
reasoning is stronger than mine, so its version is now what the code says.** Three reasons in order
of weight: (1) the pdfium sites are **not broken** (333 < 360), so an assertion reddening CI over a
non-defect is a false alarm and the first thing anyone would do is switch it off; (2) both
alternatives are worse *inside this PR* — loosening the threshold to 0.075 makes it a **description
of the status quo** rather than a requirement, and tightening ci.yml's 150 to 140 would be an
unmeasured behaviour change to the release-critical fetch path made from inside a ticket about a
different workflow; (3) the reason is recorded **in code**, where the next person to touch the guard
reads it, not buried in a ticket — the same standard CPE-1824's deleted note was held to.

**On the 10% threshold**: defensible as an interim and deliberately **not** measured before merge.
The strongest evidence it is not tuned to flatter its own subject is that the two sites it guards sit
at `260/300` = **13.3% margin**, comfortably clear rather than grazing — a threshold chosen to pass
its own work would sit just under 13.3%. It also errs **strict**: a wrong interim value produces a
loud red, never a silent pass. Both arguments are now recorded on `MIN_MARGIN_FRACTION` itself.

Handed to **CPE-1860** (no action here): whether 27s is adequate margin for ci.yml's pdfium steps,
and that `MIN_MARGIN_FRACTION` is a pure fraction with no absolute floor, so on a site with a small
cap 10% could be only a few seconds.

### One more correction, not asked for but found on the way

The ticket's premise is that this file's `--max-time` comment is accurate, and it is. But two
*other* comments in the same file made a false claim about the very flag this ticket is about:

> `curl --retry` only retries connection-level failures, never an HTTP status already received

That is wrong, and the inline copy at `head_check()` contradicted itself in its own next clause
("a transient 000/403/5xx is given a few chances to resolve itself"). curl's `retry.md` defines a
transient error as *"a timeout, an FTP 4xx response code or an HTTP 408, 429, 500, 502, 503, 504,
522 or 524 response code"* (fetched from curl's repo). So 429 and the 5xx family **are** retried.

The conclusion those comments were written to support — *a real 404 is never retried away and a real
200 is never manufactured* — happens to survive, because 404 and 200 are both outside curl's
transient set, and the three-way 200/404/inconclusive verdict is therefore still sound. But the
*reason* did not survive, and CPE-1824's entire blocker was a wrong reason left standing because its
conclusion looked right. Both comments now quote curl's actual definition, state the surviving
property and why it holds, and note that a 403 is **not** in curl's set and so goes straight to the
inconclusive branch on first response. No behaviour change.

### Gates

- `npx vitest run` — round 1: **324 files / 4315 tests**. Rounds 2 and 3: **324 files / 4322 tests**,
  all passed (guard 15 → 26).
- `npm run check` — **0 errors, 0 warnings**.
- **Real YAML parser**: every `.github/workflows/*.yml` parsed with **PyYAML 6.0.3**, not only the
  repo's bounded-subset parser — 6/6 OK. The structure was re-read *through PyYAML* to confirm the
  caps and call counts independently of the guard: `check-pins` job cap 30, both steps
  `timeout-minutes: 5`, five `head_check` invocations.
- Every `run:` block in **both** touched workflows extracted and passed `bash -n` — **65/65**
  shell-syntax OK across `ffmpeg-pin-freshness.yml` and `ci.yml` (round 3 added ci.yml to the
  sweep), so the reflowed continuation is valid shell and not just valid YAML.
- Line endings: all four touched files are CRLF in the working tree (`core.autocrlf=true`). Verified
  byte-level after every edit — CR count == LF count **at round-3 head**: 581/581
  `ffmpeg-pin-freshness.yml`, 1500/1500 `ci.yml`, 674/674 test file, 501/501 ticket. No BOM on any
  (first three bytes are real content), trailing `\r\n` intact, and `git diff --numstat` localised
  rather than whole-file, which is what an EOL rewrite would have looked like: **11/5, 65/11,
  286/17, 455/13** cumulative against `main`. (Round 2 reported 65/11, 256/17, 365/12 and
  CR 412/412 for the ticket — accurate when written, stale by the time it was read. Same class as
  finding 2 above; refreshed here, and inherently self-referential since editing this line changes
  the number it reports.) No `sed -i` and no PowerShell write touched any repo file; every scripted
  edit (the two `stripShellComment()` mutations, reverted from a byte copy) went through Python with
  explicit `newline=''` so CRLF was preserved, and was verified by the counts above afterwards.
- Positive control re-run at round-2 head through the final flag set: `pdfium-linux-x64.tgz` →
  exit 0, HTTP 200, 1,919 ms, **3,650,783 bytes**, 49 tar members, `lib/libpdfium.so` present —
  byte-identical to round 1 and to the Reviewer's independent run.

### Not verified

- **No CI run of `ffmpeg-pin-freshness.yml` itself.** It is `schedule` + `workflow_dispatch` only,
  so pushing this branch does not exercise it; PR CI validates the guard and the type-check, not the
  workflow's live behaviour. The end-to-end evidence above is a local execution of its extracted
  `run:` script, which is the closest available substitute but is not a GitHub-hosted runner.
- **The `timeout-minutes` kill was simulated**, with `timeout --signal=KILL 300`, not observed on a
  real runner. GitHub's cancellation is not literally `SIGKILL` at the process group, so the
  *manner* of the kill is approximate; the *timing* and the "only 2 of 5 assets reported" outcome
  are what the test establishes.
- **The measurements are Windows/mingw curl 8.21.0**, against loopback. The runner is
  `ubuntu-latest` with a different curl build. The per-attempt-reset behaviour is documented and
  build-independent, and CPE-1824 saw the same arithmetic on its sites, but the exact millisecond
  figures are this machine's.
- ~~**Sizing is still not machine-checked.**~~ **CLOSED in round 2** — see "The arithmetic assertion"
  above. Round 1 recorded this gap and declined to fix it as disproportionate; the Reviewer then
  demonstrated it was live on both terms of the product, and the fix turned out to be ~30 lines on
  top of what the round-1 tests already parsed. Recorded rather than deleted, because "we knew and
  chose not to" is the judgement that was wrong, and that is the reusable lesson.
- **The arithmetic assertion's margin fraction is a judgement, not a measurement.** 10% of the cap
  (30s at these sites) is argued from the steps' non-curl work being milliseconds, not derived from
  observed step durations the way CPE-1824 sized its `timeout-minutes` caps against `gh api` history.
  It is the reason `ci.yml`'s pdfium sites (7.5%) are excluded rather than folded in, so it is doing
  real work and should be revisited with data if that follow-up is taken.
