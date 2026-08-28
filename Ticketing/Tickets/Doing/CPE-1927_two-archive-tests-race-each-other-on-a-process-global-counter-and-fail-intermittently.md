---
id: CPE-1927
title: two `archive.rs` tests race each other on a process-global counter and shared session root — an intermittent failure that passes on rerun
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

A worker on CPE-1896 hit **a single `cpe-server` lib-test failure that did not reproduce in two
subsequent runs**, and could not capture the name. PR #1043's independent Reviewer went looking for
the shape and found a well-supported candidate — in a file CPE-1896 never touched.

`crates/server/src/archive.rs` has two process-global statics:

- `archive.rs:662` — `static EXTRACT_SEQ: AtomicU64`
- `archive.rs:1608` — `static SESSION_ROOT: OnceLock<PathBuf>`

libtest runs lib tests **in parallel inside one process**, so these are shared across concurrently
running tests. Two of them contend:

**`row1_a_squatted_temp_directory_is_stepped_over_not_written_into` (`archive.rs:4965`)** snapshots
`EXTRACT_SEQ` at `:4999`, then pre-creates `e{seq}` for a 64-wide block inside the **live** session
root — while other threads are incrementing that same counter and creating those same names. It
retries only `STAGE_ATTEMPTS = 5` times (`:4970`). Its own doc comment at `:4941-4943` already says
so: *"There was no announce mechanism, and `EXTRACT_SEQ` is shared with every sibling test that…"*.
A bounded retry against a counter another thread is moving is exactly "fails once in a while, passes
on rerun".

**`cpe_1786_many_extractions_add_one_directory_to_the_shared_root` (`archive.rs:5079`)** performs 25
extractions and asserts `parents.len() == 1` and `dirs.len() == 25` into the same shared session root,
against `MAX_LIVE_EXTRACTIONS = 512` (`archive.rs:1575`) — also process-global and consumed by every
concurrent extraction test.

## Candidates the Reviewer ruled out

Recorded so the next person does not re-derive them:

- `fsutil::scratch_dir` — names are already `tag-<pid>-<counter>`; verified in output.
- `shell_menu.rs:705`'s `set_var("HOME")` — guarded by `HOME_ENV_LOCK`, and Linux-only.
- `transfer.rs:1237`'s fixed `cpe-gj-base-dir` — never touches the filesystem; `guarded_join` is pure
  path math.
- `CPE_ARCHIVE_TEMP_TTL_SECS` — nothing in the suite sets it, so the aggressive foreign-session sweep
  is not in play.

## Why it matters beyond the flake

An intermittent red that passes on rerun trains the crew to re-run rather than read. This repo has
spent the night finding guards that prove nothing; a test that fails for a reason unrelated to what
it pins is the same disease from the other end — and it burns a full CI cycle (~1 hour on this repo)
each time it fires.

## Acceptance criteria

- [x] **Reproduce it first.** Run the `archive` module's tests repeatedly (and under load — this
      machine runs a dozen agents during a sprint, which is when it fired) until it fails, and capture
      the actual failing test name and message. The Reviewer was explicit that this is the
      best-supported candidate, **not a diagnosis**; do not fix on the strength of the analysis alone.
- [x] Fix the contention properly rather than by widening `STAGE_ATTEMPTS`. Options worth weighing:
      give the contending tests their own session root; make `EXTRACT_SEQ` reservations atomic against
      the directory creation; or serialise just these tests behind a lock the way `shell_menu.rs`
      already does with `HOME_ENV_LOCK`. Retry-count inflation hides the race, it does not remove it.
- [x] Whatever the fix, it must go red against the racing shape — a test that only passes because the
      race is rare is not a fix.
- [x] Sweep `crates/server` for the same pattern: any other `static` counter or `OnceLock` root that a
      test both reads and writes while siblings run in parallel.

## Notes

Filed 2026-08-27 by the sprint Foreman. Origin: an unresolved observation in CPE-1896's Work Log,
diagnosed by PR #1043's Reviewer while re-reviewing an unrelated change. `archive.rs` is **not**
touched by CPE-1896.

## Work Log

**2026-08-27 — worked. Toolchain: WSL `~/lintools`, 32 cores, `cargo test` default parallelism unless
stated; every run preceded by `find . -name '*.rs' -exec touch {} +`.**

### 1. Reproduction — measured, and it does not say what the ticket predicted

The ticket's stated symptom is an intermittent **red**. That did not reproduce, in **0 of 290 runs**:

| run shape | runs | reds |
|---|---|---|
| `archive::` module, default parallelism | 30 | 0 |
| `archive::` module, default parallelism, 24 busy-loop hogs pinning the CPU | 200 | 0 |
| `archive::` module, `--test-threads=1` | 25 | 0 |
| whole `cpe-server` lib suite (2410 tests), default parallelism | 7 | 0 |

I did not run the whole suite serially before the fix: `--test-threads=1` removes the contention by
construction, so it can only produce green, and 65 s × N of it would have measured nothing. The serial
runs below are on the `archive::` module, which is where both named tests live.

So I instrumented instead of guessing: a temporary stderr probe printing `start`, `end`, `landed`,
`ours` (how many of the 64 squatted names the test actually created) and `proven` on every attempt of
`row1_a_squatted_temp_directory_is_stepped_over_not_written_into`. **That reproduced the race
immediately**, and it is real:

- **`--test-threads=1`, 25 runs: `start=37 end=101 landed=101 ours=64 proven=true`, bit-identical
  25/25.** Fully deterministic.
- **Default parallelism, `archive::` only, 40 runs:** `start` varied run to run; `ours=63` (a sibling
  had already taken one of the 64 names, so no link could be planted in it) in **4/40**; one attempt
  raced clean past the block (`proven=false`) in **1/40**.
- **Default parallelism, whole lib suite, 7 runs:** `ours < 64` in **2/7**; one run planted only
  **62 of 64** links *and* was raced **20 names** past the block
  (`start=3 end=67 landed=87 ours=62 proven=false`), burning an attempt.

That is the same fixture defect the ticket describes, with one correction: **it cannot go red.** The
`landed_seq >= end` assertion is unreachable by this race, because `EXTRACT_SEQ` is monotonic — every
name a sibling steals inside the block is a name the counter has *already passed*, so the test's own
extraction can never be handed it again. What the race actually costs is **coverage, silently**: names
in the squat block go unarmed with no signal, and five consecutive raced-out attempts end in
`skip_notice!`, which is a **passing test**. Recorded here so nobody re-derives it — and it is worth
saying plainly that this is the *same* disease as the rest of the sprint's docket, entered from the
other end: not a guard that proves nothing, but a guard that quietly stops proving what it says.

**One caveat on the monotonicity argument, for the next editor.** It assumes `SESSION_ROOT` holds the
**private** `s<pid>-<random>` root, which only this process numbers into. `session_root()` has a
documented degraded fallback — `claim_session_root(&root).unwrap_or_else(|| root.clone())` — under which
the session root becomes the **shared, cross-process** `%TEMP%/cpe-archive` itself, and extractions are
numbered directly under it. In that mode another process can create *and remove* a name we later draw, so
`landed < end` becomes reachable and "it cannot go red" stops holding. That is not a realistic suite
condition (claiming the root has to fail outright) and it is not the shape this ticket is about, so
nothing here depends on it — but the argument above is conditional on the normal path, and saying so is
cheaper than having someone rediscover it from a red.

I could not reproduce a red, so I have not diagnosed CPE-1896's one-off failure and this ticket should
not be read as having closed it. What it closes is the coupling PR #1043's Reviewer identified.

### 2. What is shared, who wins, and whether production shares it

`EXTRACT_SEQ` (`AtomicU64`) and `SESSION_ROOT` (`OnceLock<PathBuf>`) are process-global, and libtest
runs lib tests in parallel **inside one process**, so every test that extracts anything draws from
both. The users are `row1_…` and `cpe_1786_many_extractions_add_one_directory_to_the_shared_root`
plus the eight `extract_archive_entry{,_any}` round-trip tests.

*Who wins:* `row1_…` reads the counter (`load`) and then acts on the value it read, so it loses to any
sibling whose `fetch_add` lands between the read and the squat — the loser observes a squat block with
holes in it (unarmed names) or an extraction that started past the block entirely. Nothing announces
either.

**Production shares them too, and correctly.** The app runs concurrent extractions through
`temp_extract_target` every time a user opens two files inside an archive; the atomic `fetch_add` plus
the exclusive `fs::create_dir` walk is precisely the mechanism built to survive that (CPE-1195,
CPE-1733), and it is what the row-1 guard exists to pin. So **serialising the tests behind a mutex was
the wrong fix specifically because production shares them**: a lock would have made the test pass in
sequence while leaving it predicting a number it does not own, and it would have quietly changed the
conditions under which the production walk is exercised.

### 3. Fix — remove the sharing, not serialise around it

`temp_extract_target` and `extract_archive_entry` are each split into a one-line public/global wrapper
plus a body that takes the namespace explicitly (`ExtractNamespace<'a> = Option<(&Path, &AtomicU64)>`;
`None` = the process globals, which is what all production callers pass). `row1_…` now builds **its
own root and its own `AtomicU64`** and drives the identical production body through them.

Consequences at the test:

- The squat is exact: `fs::create_dir` on all 64 names now `unwrap()`s (in a private root a name we
  cannot create is a fixture bug, not a shrug), and `stage_live_link` is **asserted** rather than
  discarded, so a partly-armed block is a failure instead of a silent weakening.
- The landing assertion **tightened from `>=` to `==`**: the extraction must land at exactly `e64`.
- The five-attempt retry loop, the `ours` bookkeeping, and the `skip_notice!` are all deleted. There is
  nothing left for the test to be lucky about.
- A final race-free leg extracts once through the *real* `extract_archive_entry` and asserts the
  directory it gets is an `e<seq>` child of `session_root()` — that keeps the private-namespace squat
  honest about staging the hazard at the address production actually numbers into. It predicts no
  number, so it cannot be raced.

`cpe_1786_many_extractions_add_one_directory_to_the_shared_root` **keeps the live globals on purpose**,
and the doc now says why: it predicts nothing, so both of its claims survive any interleaving by
construction (`fetch_add` hands out each number once ⇒ N distinct dirs; `SESSION_ROOT` is a `OnceLock`
⇒ one parent). Isolating it would make it *vacuous* — "all extractions share one session root" is a
claim about the process-global root, so measuring it anywhere else measures nothing.

### 4. Red-proof

Two sabotages, each 30× under default parallelism:

| sabotage | result |
|---|---|
| `create_dir` → `create_dir_all` in `temp_extract_target_in` (the real CWE-377/CWE-59 bug row 1 guards) | **30/30 red**, on `e0`, victim assertion naming the damage |
| the injected namespace ignored (fall back to the process globals) | **30/30 red** on the landing assertion — the isolation is load-bearing, not decoration |

**Read the first 30/30 as a floor check, not as this PR's delta.** The round-2 Reviewer ran that same
sabotage against **`main`'s current (v2) test** and got **30/30 red (module), 8/8 red (full suite)**. So
v2 already caught this sabotage, and the rewrite does not turn a leaky one into a caught one — it shows
only that the rewrite **did not lose** the catch. The "green in 2 of 3 runs" figure is PR #906's, against
**v1**, which `main` has not carried for a long time; my round-1 wording said "the v1 test" and was
accurate, but sitting under a *Red-proof* heading it read as a gain and was repeated as one in a status
report. It is not a gain. Correcting it here rather than softening it: this is the
measured-elsewhere-reads-as-measured-here family the sprint has spent the day burning down, and a
red-proof table is the worst possible place to leave one.

**The genuine delta is determinism and coverage**, which are the numbers in §1 and are mine. v2 passed
whether or not it proved anything: it silently lost names out of its squat block in **2 of 7** full-suite
runs here, and the round-2 Reviewer — running the harder shape, 124 attempts under 24 CPU hogs — measured
`ours < 64` in **13 of 124 attempts (~10%)**, i.e. a partly-armed block with **no signal**, more often
than my own figures showed; it also reproduced the raced-clean-past shape directly at
`start=83 end=147 landed=148 ours=64 proven=false`. Both shapes are unreachable now: no retry, no `ours`
bookkeeping, no `skip_notice!`, all 64 names armed on every run, and the landing assertion tightened to
`==`. The Reviewer also confirmed **0 reds in those 124 attempts**, which is the other half of the §1
finding — the fixture defect degrades coverage, it does not produce the intermittent red the ticket
predicted.

### 5. Sweep (AC 4) — one other instance fixed, one cleared, and a corrected criterion

**Corrected criteria (round 2).** My round-1 criteria were *"every `static` in `crates/server/src`"*
**and** *"every `.load(Ordering::…)` in test code"*. The second half is wrong, and wrong in the
repo's own [[enumerate-dont-recall]] way: **a `Mutex`/`RwLock` global has no `.load`**, so the
`and` silently excluded a whole family of shared globals — the CPE-1932 failure mode operating
*inside the sweep built to catch it*. The criterion that actually covers AC 4 is:

> every **process-global** in `crates/server/src` — `static` of any interior-mutability flavour
> (`Atomic*`, `OnceLock`, `Mutex`, `RwLock`, `LazyLock`) — that **test code reads or writes by any
> means**, not only via `.load`. `thread_local!` is excluded by construction (per-thread), and a
> test-local `Arc<Atomic*>` is not a global.

Re-run on that criterion the enumeration is: all `thread_local!` `Cell`s (`batch_media`,
`vault_manager`, `batch_execute`) are per-thread, not shared; every `.load` outside `archive.rs` is on a
test-local `Arc<Atomic*>`; `fsutil::scratch_dir`'s `SEQ`, `shell_menu`'s `HOME_ENV_LOCK` and
`transfer.rs`'s fixed base dir were already ruled out by PR #1043's Reviewer and I re-confirmed them.
That leaves one hit that the broken criterion had hidden, and one real fix.

**Cleared: `archive.rs:1620` `LIVE_EXTRACTIONS`** — a process-global
`Mutex<VecDeque<(PathBuf, Instant)>>` that production writes on **every** extraction, and that
`cpe_1786_the_live_queue_is_monotonic_so_the_front_is_never_newer_than_the_back` (`:5842`) **reads while
siblings run in parallel**. This is exactly the target shape and it should have been in the round-1
report. Verdict after reading it: **safe, and safe by construction rather than by luck** — but for a
sharper reason than "it filters on its own root", because its *ordering* assertion deliberately walks
sibling entries too:

- **Ordering claim.** `note_extraction_dir` samples `Instant::now()` **inside** the same critical section
  that performs the `push_back`. Timestamps therefore enter the deque in the order the mutex grants, and
  a sibling physically cannot interleave an older timestamp behind a newer one. The non-decreasing
  property is enforced by the lock. (Had the instant been sampled *before* acquiring the lock, this test
  would be a genuine flake: thread A samples `t1`, is descheduled, B samples `t2 > t1` and pushes, A then
  pushes `t1` — queue goes backwards. Noted at the site so nobody hoists it.)
- **Count claim.** `seen` filters on the test's own scratch root and asserts `>=`, so sibling pushes can
  only add entries it ignores. The only operation that could *subtract* is eviction, and `drain_reapable`
  cuts only above `MAX_LIVE_EXTRACTIONS` = 512 (and only after a `REAP_GRACE` ten-minute quiet gap) or
  above `HARD_CAP_EXTRACTIONS` = 4096. The whole lib suite pushes on the order of tens.

So: no change, deliberately — and the reasoning is now written at the site rather than living only in a
reviewer's head. Both facts were verified by reading `note_extraction_dir` and `drain_reapable`, not
inherited.

One real hit:

- **`ffmpeg_util::set_native_dep_dir_is_a_silent_no_op_on_a_second_call`** writes the process-global
  `NATIVE_DEP_DIR: OnceLock<PathBuf>` that production's `resolve_ffmpeg_bin` reads, while siblings run
  in parallel — and asserted **nothing at all** (*"nothing else to assert"*), in a test whose entire
  subject is a state change. Left on the global (same reason as `cpe_1786_many_…`: the `OnceLock` *is*
  the subject) but made non-vacuous — it now reads the state back and asserts the second call did not
  move it, phrased against whatever the first call observed so it cannot itself become order-dependent.
  Red-proofed: with `set_native_dep_dir` neutered the test fails; before this change that sabotage was
  green.
  **Scope caveat:** `pub mod ffmpeg_util` is `#[cfg(any(feature = "video-thumb", feature = "waveform"))]`
  (`lib.rs:536`), so this test — and therefore this fix — **exists in only one of CI's three feature
  modes**, `pdf-thumb,video-thumb,waveform,dicom-thumb`. It guards nothing in the default and
  `--features index` modes, because the module it lives in is not compiled there. That is correct (the
  seam under test does not exist without ffmpeg either) but it is not what "fixed" implies unqualified,
  so: one mode of three.
- `thumb_pdf`'s identical `NATIVE_DEP_DIR` has no test writer at all — noted, nothing to fix.

### 5b. A behaviour change round 1 did not report — `session_root()?` ordering

Splitting `temp_extract_target` into wrapper + `temp_extract_target_in` moved `session_root()?` **in
front of** the `file_name()` validation; the old single function validated first. So an entry with no
`file_name()` (`..`, `.`, `""`) now claims the session root and can run the stale-session sweep before
returning the same `"invalid entry name"`. Round 1 missed it, and calling the production path
"byte for byte" without this qualifier was too strong: the *walk* is byte-for-byte, the *evaluation
order* is not.

**Decision: note it, do not reorder** — and the argument is the ticket's own thesis rather than
convenience.

- *Why it is benign, precisely.* Nothing derived from `inner` reaches `session_root()`; it builds
  `%TEMP%/cpe-archive/s<pid>-<random>` from our own pid and RNG only. No attacker-controlled path is
  created, and the extraction still refuses. The one externally visible difference is the error
  **string**: if the shared root is a link, `refuse_link_at_new_file` now answers first, which is the more
  informative refusal — it names a real environmental hazard instead of masking it behind a name
  complaint. The change is therefore a wash at worst and a small improvement at best.
- *Why not reorder, given it is a two-line fix.* **The ordering is not pinnable by a test in this suite.**
  Its only observable is a once-per-process side effect on `SESSION_ROOT`, a `OnceLock` every sibling test
  races to initialise; a test asserting "`session_root()` was not called" would have to read that global
  mid-suite and would be exactly the shared-mutable-fixture shape this ticket exists to delete. The other
  route — making the shared root un-claimable to force the error path — mutates `%TEMP%` for the whole
  process. So a reorder here could only ship **unverified**, in a security-adjacent refusal path, in the
  round-2 pass of an approved PR. Shipping an unpinnable edit and shipping an unpinnable guard are the
  same mistake; the rule this sprint has been enforcing all day says take the honest comment instead.
- *What is written down.* The full note lives at the `temp_extract_target` site, including the recipe if
  it ever does need reordering (have the wrapper pass an `ExtractNamespace` and let the body resolve the
  root after validating) and the trigger that would make it worth doing: `session_root()` acquiring a
  caller-visible cost, or a side effect derived from `inner`.

### 6. Verification

`crates/server`, all three feature modes CI runs, clippy `--all-targets -D warnings` **clean** and
tests green in each: default (**2410 passed**, unchanged from the pre-change baseline of 2410 — the
suite size delta is **zero**, one test in and one test out), `--features index`, and
`--features pdf-thumb,video-thumb,waveform,dicom-thumb` (2465 passed). Plus `archive::` 60× at default
parallelism and 15× at `--test-threads=1`, all green, and a Windows-native `cargo test --lib
archive::tests::` (102 passed) since CI runs a 3-OS matrix.
