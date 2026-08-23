---
id: CPE-1863
title: the retention byte-cap loop destroys checkpoints while reclaiming nothing
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

`snapshot_prune::apply`'s byte-cap loop tracks progress as `total = total.saturating_sub(freed)`. When a
prune frees **nothing**, `freed == 0`, so `total` never falls, the cap is never seen as met, and the loop
runs all the way to its `kept.len() <= 1` floor.

Measured by the independent Security Auditor during CPE-1861, on a store with **no tamper at all** — six
identical captures, so every blob is shared and pruning any one manifest frees nothing:

```
apply(cap = total - 1) -> kept = 1, pruned = 5, bytes_freed = 0
```

Five checkpoints destroyed. Zero bytes reclaimed. The cap it was trying to meet was never going to be met
by deleting them, because they were not what was using the space.

## Reachability

**Not reachable from the app today.** `snapshot_run_due` passes `None` for the byte cap, and no caller in
`src/` passes `maxTotalBytes`. The behaviour is byte-identical on `main` — CPE-1861 neither introduced it
nor changed it, though that ticket widens the set of stores where `freed == 0` is the normal outcome.

So this is a trap waiting on whoever wires the byte cap to a setting, not a live defect. It is filed
because the loop reads as correct and the failure is silent: it reports success, having deleted the user's
history without helping.

## Acceptance criteria

- [x] A pass that frees nothing must stop rather than continue to the floor. Decide what "no progress"
      means and record it — the honest reading is that if pruning the oldest candidate frees zero bytes,
      pruning the next one probably will not either, because shared blobs are shared by everything.
- [x] `bytes_freed: 0` with a non-empty `pruned` list should be surfaceable as the anomaly it is. Note
      that **nothing in `src/` consumes `RetentionApplyResult` at all** (only `bindings.gen.ts` names it),
      so today there is no consumer to surface it to — say whether that is in scope here or belongs with
      CPE-1862.
- [x] Test the no-tamper fixture above: identical captures, a cap below the total, and assert the loop
      does **not** run to the floor. Red-proof it — it must fail today.
- [x] Check the interaction with CPE-1861's accepted leak: a store containing an ignored manifest file has
      blobs pinned that no prune can free, so `freed == 0` is the *expected* outcome there. The fix must
      not turn that into a stall.
- [x] Say what the loop should do when the cap genuinely cannot be met — stop and report, or prune to the
      floor and report honestly that the cap was not met. Reporting success either way is what this ticket
      is about.

## Notes

Found by the independent Security Auditor during CPE-1861's audit, which recommended merge and filed this
as one of four non-blocking follow-ups. Its own framing: pre-existing shape, widened set.

Read CPE-1861's Work Log first — its `manifests_naming` witness is what decides whether a blob is freeable,
and this loop's behaviour depends entirely on that answer.

Related: CPE-1861 (the witness), CPE-1844 (`index.json` steering the same retention decision), CPE-1862
(the unreconciled index in the same subsystem).

## Work Log

### The measurement, reproduced before anything changed

The ticket's figure is exact. Reproduced here as the new headline test against the unmodified loop (the
red-proof below), six identical 400-byte captures a day apart, GFS keeping all six, `cap = total - 1`:

```text
HARM: the byte cap destroyed 5 of 6 checkpoints to reclaim nothing — ["1787480647979.json"]
```

One manifest file left of six, `bytes_freed = 0`, `Ok`. No tamper anywhere in the fixture: this is what a
store looks like when a scheduled capture runs over a folder nobody edited.

### "No progress", defined — and why it is not `freed == 0`

**No progress = the re-measured store footprint did not strictly fall across an eviction** (`after <
total`, where both come from `store_total_bytes`). Not `snapshot_capture::prune`'s return value.

They normally agree, and `freed == 0` is the common instance, but they answer different questions and the
difference decides a real case. `prune` credits the bytes of the blob *files it actually removed*. This
module's documented failure direction is leak-over-corruption: a blob whose last namer was pruned but
whose file could not be deleted is credited **0** by `prune` while genuinely leaving the *reclaimable*
footprint — which is what CPE-1844 made `store_total_bytes` measure, and what the cap is compared against
— smaller. Stopping on `freed == 0` there would abandon a loop that was working. The cap is a statement
about `total`, so progress has to be measured in the same currency. CPE-1844's re-measure at the foot of
the loop is exactly the reading the rule needs; that line is now load-bearing for two tickets.

### What the loop does when the cap cannot be met: stop, and say so

**Stop and report** — never prune to the floor chasing a cap. `RetentionApplyResult` gains
`byte_cap: ByteCapOutcome`, a four-variant discriminant:

| variant | meaning |
|---|---|
| `not_requested` | no cap passed (`None`, or `Some(0)`) — every caller in the app today |
| `met` | the measured footprint is at or under the cap |
| `stopped_no_progress` | an eviction reclaimed nothing; the loop stopped. **Cap not met** |
| `stopped_at_floor` | out of evictable survivors (one snapshot is always kept). **Cap not met** |

Before this, "the cap was met" and "the cap could not be met and we destroyed checkpoints discovering
that" were the same answer — `Ok` with a non-empty `pruned`. A caller reading `pruned` alone reports
success either way, which is the whole of the ticket. `ByteCapOutcome::cap_missed()` is a convenience for
phrasing the two `Stopped*` cases, explicitly *not* the discriminant: which of the two it is decides what
the user can do about it.

**Why not CPE-1845's `OpOutcome`** — checked, as instructed, rather than assumed. That enum answers "what
happened to *this one item*, and can the user retry it" for a bulk per-path operation. Every manifest this
loop deletes is unambiguously `Applied`: the item succeeded. What is unresolved is the *budget the
deletions were justified by*, which has no item at all. Mapping a missed cap onto
`SkippedByPlan`/`HeldBackByCheckpoint` would report a hold-back for operations that were in fact
performed, and `HeldBackSummary` is a summary of *held-back items* — there are none here. So a second
vocabulary, deliberately, but its *conventions* are reused because those are the reusable part: a
discriminated union rather than a prose prefix, `snake_case` on the wire, variants chosen by the
user-facing decision they drive rather than by internal control flow.

### Two costs, stated here rather than discovered later

1. **One checkpoint is still destroyed.** Nothing in the loop can know a prune's yield without performing
   it — the yield is "blobs no other manifest *file* still names", which is `prune`'s own witness scan,
   run after its point of no return. So the headline fixture loses one checkpoint instead of five. The
   strictly better fix is a **predictor**: ask what evicting a candidate *would* free and skip a
   zero-yield candidate without deleting it. That needs a witness-with-exclusion query alongside `prune`'s
   plus a model of its `refs <= 1` at-risk gate, and CPE-1861's whole lesson is that a mirrored predicate
   which drifts is worse than the bounded loss. Left as a follow-up; the residual is not silent, it is
   `stopped_no_progress` with `bytes_freed == 0`.
2. **A later candidate might have freed bytes.** `m1={A}, m2={A,B}, m3={A}` — evicting m1 frees nothing
   while evicting m2 would free B. The loop stops at m1 and reports the cap unmet. Deliberate direction:
   continuing spends *certain* destruction of the user's history on a *speculative* reclaim, and the usual
   reason an eviction freed nothing is that the blobs are shared with everything, in which case continuing
   destroys the lot for nothing. Reporting honestly costs the user a decision; guessing costs them their
   checkpoints. The rule is also **per pass** — it holds no state across calls — so a repeating schedule
   can still erode a store one fruitless eviction at a time. That is bounded, visible in the outcome, and
   the predictor above is what removes it.

### The CPE-1861 interaction: a stop, not a stall

CPE-1861 accepted a residual: a manifest file `list_manifests` refuses — an Explorer `"<id> - Copy.json"`,
the ~122-byte witness, invisible to the planner and permanent — still counts as a namer to `prune`'s
witness *and* to `store_total_bytes`. Its snapshot's blobs are therefore pinned, counted toward the cap,
and reclaimable by no prune retention can make. `freed == 0` is the **expected** outcome there, so the new
rule fires on it by construction and the ticket is right to ask whether that becomes a stall.

It does not, and `cpe_1863_an_invisible_manifest_pinning_blobs_stops_the_cap_without_stalling_it` pins all
three halves of the answer. Three distinct 200-byte captures plus a copy of the oldest, `cap = 1`:

```text
pass 1   pruned=[m1]  bytes_freed=0    byte_cap=stopped_no_progress   planner sees 2   (was: floor, 1 left)
pass 2   pruned=[m2]  bytes_freed=200  byte_cap=stopped_at_floor      cap still unmet
```

`apply` returns `Ok`; the GFS half of the pass has already run in full; nothing is wedged; and pass 2
proves the rule stops a *fruitless* walk without stopping a *productive* one. The pinned blob is still
there, still unreclaimable, still counted — unchanged by this ticket, and now **named** by the outcome
rather than paid for in checkpoints.

### The `bytes_freed: 0` anomaly — what is in scope here, and what is CPE-1862's

In scope and done: the backend can no longer *report* it as a success. `byte_cap` names the byte-cap case
as the anomaly it is, and `bytes_freed`'s doc comment records the general shape.

Out of scope, and it belongs with **CPE-1862**: showing it to anyone. `RetentionApplyResult` has **no
consumer in `src/` at all** — verified, `bindings.gen.ts` is the only file in the frontend that names it,
and `snapshot_run_due` passes `None` for the cap so the field is `not_requested` on every path the app
takes today. Inventing a UI for a value nothing reads would be a second guess on top of the first. CPE-1862
is the ticket that builds the reconcile/report path over this same subsystem; the discriminant is ready
for it.

Worth recording deliberately: `bytes_freed == 0` after a **GFS-only** pass is *not* an anomaly. The user
asked for fewer checkpoints, not for fewer bytes, and a policy pass that frees nothing did what it was
asked. The anomaly is only anomalous when the deletion was justified *by bytes* — which is why the
outcome is scoped to the cap rather than bolted onto the result as a bare boolean.

### Red-proofs — every one a single line, observed red, reverted

All against the finished tree. Baseline `cargo test --lib snapshot_prune`: 20 passed.

| # | line changed | from → to | result |
|---|---|---|---|
| 1 | `snapshot_prune.rs` loop | `let progressed = after < total;` → `after <= total;` | **2 red** — restores `main`'s exact behaviour. `HARM: the byte cap destroyed 5 of 6 checkpoints to reclaim nothing` and `HARM: the cap walked to the floor over blobs no prune of it could ever free` |
| 2 | same line | `after < total` → `false` | **3 red** — the over-tightening pin: `HARM: the no-progress rule stopped a loop that was reclaiming bytes`, plus the floor and CPE-1861 tests. Note `cpe_1844_the_byte_cap_still_thins_a_store_that_is_genuinely_over_it` stays **green** here (it only ever needs one eviction) — which is exactly why the multi-eviction pin had to be written |
| 3 | floor arm | `break ByteCapOutcome::StoppedAtFloor;` → `Met;` | **2 red** — `HARM: a cap the store cannot reach was reported as met` |
| 4 | `let mut byte_cap = ByteCapOutcome::NotRequested;` | → `Met` | **1 red** — a pass with no cap must not claim one |
| 5 | headline fixture | capture loop line writes `vec![b'a' + i as u8; 400]` per iteration | **1 red** — `LIVE: 6 blob files for 6 checkpoints — nothing is shared, so a prune WOULD free bytes and this fixture does not test no-progress` |
| 6 | headline fixture | `daily: 100` → `daily: 1` | **1 red** — `LIVE: the GFS pass wants to prune on its own, so this fixture does not isolate the byte cap` |

5 and 6 are the **liveness** proofs, and they are proofs of the *helpers*: both messages come from
`live_cap_fixture` / `assert_blobs_are_shared`, which every CPE-1863 test routes through. Folded into two
helpers rather than written out per test on CPE-1844's evidence, where per-test liveness checks let a
decoy-sibling trap invert a claim from 2-passed/9-failed to 9-passed/2-failed with three tests certifying
nothing. One helper cannot rot in three places.

### Gates

| gate | result | delta |
|---|---|---|
| `crates/server` clippy `--all-targets -- -D warnings` | clean | — |
| `crates/server` `cargo test` lib | 2361 passed, 4 ignored (2365 total) | **+5** (was 2360) |
| `crates/server` integration bins | 21 / 22 / 2 / 1 / 1 / 45 / 16 / 32, `ticket_mcp` 0, doc-tests 0 — all pass | 0 |
| `src-tauri` clippy default | clean | — |
| `src-tauri` clippy `--features sidecar-platform` | clean | — |
| `src-tauri` `cargo test` default | 214 passed | 0 |
| `src-tauri` `cargo test --features sidecar-platform` | 269 passed | 0 |
| `npm run check` | 0 errors, 0 warnings | — |
| `vitest run` | 328 files, 4390 tests passed | 0 |
| `bindings.gen.ts` | regenerated (`export_bindings --features "specta-bindings sidecar-platform"`), +61/-2 | — |

`bindings.gen.ts` was regenerated deliberately and not as an afterthought — CPE-1844 tripped the
Typed-bindings drift guard on exactly this, and this change adds a `specta::Type` enum *and* a field.

### Docs

`src/docs/16-checkpoints.md` gains **"When a size limit can't be met"**, under the existing "How big the
store thinks it is". It explains sharing in plain language, states the old behaviour and its cost (five of
six for nothing), the new stop-and-say-so, and both practical notes — that one snapshot is still deleted
first, and that a stray record file holding the space is fixed by deleting that file, not by more cleanup.
No new `Section`, so `sectionDocs.ts` is untouched.

### Not verified

- **Reachability is unchanged: still not reachable from the app.** `snapshot_run_due` passes `None` and no
  caller in `src/` passes `maxTotalBytes`, so every field this ticket adds reads `not_requested` in the
  shipped app. This is a trap disarmed for whoever wires the cap to a setting, not a live fix, and it has
  had no GUI exercise because there is no GUI path to exercise.
- Non-Windows behaviour is CI's word, not measured locally — the fixtures assert relative byte counts
  (`400`, `4 x 200`) rather than absolute filesystem sizes, which is the shape that has broken on the
  Linux/macOS legs before.
- The predictor described above is *not* implemented, so "no checkpoint is destroyed for nothing" is not
  claimed — only "at most one, and it is reported".
