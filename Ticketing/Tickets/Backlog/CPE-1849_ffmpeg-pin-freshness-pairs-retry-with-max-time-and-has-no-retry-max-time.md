---
id: CPE-1849
title: ffmpeg-pin-freshness pairs --retry with --max-time and has no --retry-max-time
type: task
priority: Low
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

## Why this is Low and not a repeat of CPE-1824

The defect CPE-1824 fixed was a **false claim recorded as verified fact** — code comments asserting
`--max-time` bounded the whole invocation. This file's own comment is already **accurate**: it says
*"--max-time bounds each attempt"*. So nothing here is lying.

The worst case is also much smaller: `3 x 30s` plus delays, not `5 x 180s`.

What remains is that the workflow has no explicit series bound, so its real worst case rests on
whatever outer `timeout-minutes` applies rather than on anything the curl line states.

## Acceptance criteria

- [x] Both sites either gain a `--retry-max-time` coherent with the step's `timeout-minutes`, or record
      why the outer backstop is sufficient here. CPE-1824's arithmetic is the precedent: curl checks the
      retry timer *before* starting each retry and lets an in-flight attempt finish, so worst case is
      `retry-max-time + max-time`, and the value must satisfy `N + max-time < timeout-minutes` with margin
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
>= 10, so it stopped. Worst case really is `retry-max-time + max-time`, not `retry-max-time`.

### Value chosen: `--retry-max-time 20`, at both sites

Sized against the **binding** step, the five-call one:

- worst case per call = `retry-max-time + max-time` = `20 + 30` = **50s**;
- five calls = **250s**, inside the 300s cap with **50s (17%) of margin**;
- the two-call site is the same 50s worst case x 2 = **100s**, 200s of margin.

Why the same value at both rather than a looser one where there is room: a larger `N` buys the
retries nothing. Their actual job here is connection-level failure (refused/reset), which fails in
milliseconds, so three retries plus their 2s delays finish inside ~6s — far under 20. A larger `N`
would only widen the stall worst case. One number, one rule, tighter site governs.

**Why the value is necessarily *below* `--max-time`, which is worth stating because it looks odd.**
It falls straight out of the multiplier, not out of any property of `--max-time`: worst case per
call is `N + 30`, so `N = 30` already puts five calls at exactly 300s — the cap with zero margin —
and anything larger is past it. Five calls under one cap is what forces `N < 30` here; the two-call
site would tolerate up to `N = 120`, which is precisely why the value must not be copied between
sites without recounting the calls.

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

Three things fall out of that one run: `logicalLines()` really does join this file's backslash
continuations (each offender is reported as one joined logical line carrying both flags); the repo's
bounded-subset `parseYaml` really does parse this file (`parseWorkflow` throws otherwise, and did
not); and `stripShellComment()`'s quote-awareness holds on real code here, because the joined tail
contains `-w '%{http_code}'` whose `#` is quoted and would otherwise truncate the line into
invisibility.

**Two assertions added beyond the pairing scan**, because for this file the sizing rests on inputs a
future edit could move without touching any curl line: `HEAD-check pinned assets` still makes exactly
five `head_check` calls under `timeout-minutes: 5`, and `Validate the recommendation` still loops
over exactly the two-element `for entry in "win64 …" "linux64 …"` list under its own
`timeout-minutes: 5`. Add a sixth asset, or drop a cap to 4 minutes, and `20` silently stops being
correct while every flag still reads fine. This closes, *for this file only*, a slice of the "the
guard enforces PAIRING, never SIZING" limitation CPE-1824 documented — it pins the **inputs** to the
arithmetic, so a change to them has to come back here and redo it. It still does not check the
arithmetic itself; the general limitation stands and its note is left intact.

Guard went 15 tests → **19** (pairing scan for the new file, non-vacuity count for the new file, two
sizing-input assertions).

**Red-proofs — four, each reverted immediately:**

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

- `npx vitest run` — **324 files / 4315 tests, all passed** (guard 15 → 19).
- `npm run check` — **0 errors, 0 warnings**.
- **Real YAML parser**: every `.github/workflows/*.yml` parsed with **PyYAML 6.0.3**, not only the
  repo's bounded-subset parser — 6/6 OK. The structure was re-read *through PyYAML* to confirm the
  caps and call counts independently of the guard: `check-pins` job cap 30, both steps
  `timeout-minutes: 5`, five `head_check` invocations.
- Every `run:` block in the touched workflow extracted and passed `bash -n` — 6/6 shell-syntax OK,
  so the reflowed continuation is valid shell and not just valid YAML.
- Line endings: the file is CRLF in the working tree (`core.autocrlf=true`). Verified byte-level
  after every edit — CR count == LF count (570/570 workflow, 472/472 test file), trailing `\r\n`
  intact, and `git diff --numstat` localised (54/11 and 81/14) rather than whole-file, which is what
  an EOL rewrite would have looked like. No `sed -i` was used on any repo file.

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
- **Sizing is still not machine-checked.** The new assertions pin the *inputs* (call count, caps);
  nothing computes `5 x (20 + 30) < 300`. A future edit that changes `--max-time 30` to something
  larger would pass every test here and break the arithmetic silently. Deliberate — an arithmetic
  assertion would have to parse the flags numerically and the value out of `timeout-minutes`, which
  is a bigger change than this Low ticket justifies — but recorded so nobody over-trusts a green run.
