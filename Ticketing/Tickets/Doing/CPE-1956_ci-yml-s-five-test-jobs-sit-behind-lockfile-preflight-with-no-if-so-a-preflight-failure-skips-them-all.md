---
id: CPE-1956
title: `ci.yml`'s five test jobs all sit behind `lockfile-preflight` with no `if:`, so one preflight failure silently skips the entire test suite
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Found by PR #1064's worker while enumerating every `needs:`-chained job in the repo (CPE-1932 —
enumerate, do not recall). It is the **second instance** of the shape that starved the agent catalog
for 33 days, in a different workflow.

`ci.yml`'s five jobs — `backend`, `crates`, `net-e2e`, `sidecar`, `msrv` — all declare
`needs: lockfile-preflight` with **no `if:`**. If the preflight fails, all five are **skipped**, not
failed. The whole test suite silently does not run.

## Why this is not the same severity as CPE-1953, and why it is still worth fixing

Nothing here publishes, so the blast radius is smaller than a catalog that stops reaching users.

But there is a specific hazard: **GitHub counts a skipped required status check as satisfied.** If any
of those five is (or becomes) a required check, a preflight failure could let a PR read as *mergeable*
with its test suite never having run.

**Important mitigating fact, verified independently this week:** this repo currently has **no branch
protection at all** — PR #1052's reviewer measured `branches/main/protection` → 404 "Branch not
protected" and `rulesets` → `[]`, with a majority of recent `main` commits being direct pushes. So
nothing is required today and the hazard is **latent**. That is exactly why it should be fixed now,
cheaply, rather than after someone turns protection on and inherits a silent hole.

Note also that a Foreman merging on a `ci-poll.mjs` verdict reads `pending`/`failure` counts — a
**skipped** job is neither, so this shape is invisible to the merge gate this crew actually uses.

## Acceptance criteria

- [ ] Add a terminal `if: always()` verdict job over the five that **fails** when any of them did not
      run, in the shape `gui-smoke-linux-verdict` (CPE-1753) already uses — that job exists precisely
      because "everything else happened to pass" is not the same as "everything ran".
- [ ] **Red-proof it**: force `lockfile-preflight` to fail and confirm the verdict job goes **red**
      rather than grey, and that the five skipping is visible in its message. Both directions — a
      genuine all-green run must still pass.
- [ ] Decide whether `needs: lockfile-preflight` is a real **data** dependency or just ordering. PR
      #1064 kept `catalog`'s `needs: release` because it genuinely needs the release object
      `tauri-action` creates; if the preflight produces nothing the five consume, decoupling may be
      simpler than a verdict job. Record which and why, in the workflow rather than only the PR.
- [ ] Extend the `needs:`-chain ratchet PR #1064 added so this instance is **recorded with a verdict**
      rather than left unclassified. That ratchet derives all 11 chains at run time; it should red if a
      new unguarded chain appears.
- [ ] While there: confirm the other two chains PR #1064 marked **accepted** (`release-sidecar`,
      `gui-smoke-linux`) are still correctly accepted after this change.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1064's enumeration, which found it and deliberately
did **not** fix it there — different workflow, different blast radius, and folding it in would have
made a release-plumbing PR touch the whole CI suite.

Related: **CPE-1953** (the same shape starving the catalog for 33 days), **CPE-1753** (the
verdict-across-all-shards job this should copy), **CPE-1932** (enumerate, do not recall — the sweep
that found it), **CPE-1934** (the ratchet registry).

## Work Log

**2026-08-27 — worked.**

### The decision, and the argument for it

Two defensible answers existed and they differ in what a PR reviewer ends up looking at:

- **(a) Decouple** — delete `needs: lockfile-preflight` from the five, let them run, let the
  preflight red on its own.
- **(b) Keep the edge, make the skip loud** — a terminal `if: always()` verdict job over the five.

**Chosen: (b).** First, the `needs:` edge was classified: it is **ordering, not data**.
`lockfile-preflight` declares no `outputs:`, uploads no artifact, and leaves nothing on disk for
`backend`/`crates`/`net-e2e`/`sidecar`/`msrv` to read — each one checks out and resolves
independently. So the edge *could* be deleted without breaking correctness. It is kept anyway,
because it is exactly what converts CPE-1932's failure mode ("one stale lockfile discovered per
hour-long 3-OS matrix run", found seven times in a row) into "every stale lockfile named in
seconds". Decoupling would spend a full matrix run of compute re-deriving a fact already
established, and report it the worst possible way — one job at a time, scattered through unrelated
red.

Second, and decisively: **decoupling would not have fixed the actual defect.** The defect is not
that the five are skipped; it is that the skip is *silent*. A job can still be skipped by a
cancellation, and the next `needs:` edge anyone adds reopens the hole. The durable fix is a terminal
gate. Recorded in `ci.yml` itself — at the `lockfile-preflight` job and at length on the new
`ci-verdict` job — not only in this ticket.

### What a PR viewer sees

Measured, not assumed: `GET /repos/:owner/:repo/branches/main/protection` → **404 "Branch not
protected"**, `/rulesets` → **`[]`**. Nothing is a required check today, so the
"skipped-satisfies-required" half of the hazard is **latent**; the "grey reads as N/A" half is live.

| | before | after |
|---|---|---|
| preflight fails | `Lockfile pre-flight` red; the five **grey/skipped**; nothing anywhere says the Rust suite did not run. "1 failed" understates it by five. | same red + same five grey, **plus `CI verdict` RED**, naming each of the five and the word `SKIPPED`, and stating that a skipped job is not a pass. |
| all green | five green | five green + `CI verdict` green (checkout + a few ms of node) |
| required checks | any of the five, if made required, is **satisfied by a skip** | `ci-verdict` runs on `always()`, so it is present and definite on every run and **cannot** be satisfied by a skip |
| `scripts/ci-poll.mjs` | a skip is neither `pending` nor `failure` — invisible to the merge gate | `ci-verdict` is a real `failure` |

### The full `needs:` enumeration (all 8 workflow files, derived at run time)

`catalog-freshness.yml`, `ffmpeg-pin-freshness.yml`, `model-snapshot.yml` and
`release-pipeline-watchdog.yml` contain **no** `needs:` edges at all. The twelve that exist:

| edge | verdict | changed? |
|---|---|---|
| `release.yml/verify-published-manifest` | guarded, `!cancelled()` (CPE-1872) | no |
| `release.yml/catalog` | guarded, `!cancelled()` (CPE-1893) | no |
| `release-sidecar.yml/release-sidecar` | accepted silent skip; covered by `verify-published-manifest-sidecar` | no |
| `release-sidecar.yml/verify-published-manifest-sidecar` | guarded, `!cancelled()` | no |
| `gui-smoke.yml/gui-smoke-linux` | accepted silent skip; covered by `gui-smoke-linux-verdict` | no |
| `gui-smoke.yml/gui-smoke-linux-verdict` | guarded, `always()` (CPE-1753) | no |
| `ci.yml/backend` | accepted silent skip; **now covered** by `ci-verdict` | covered |
| `ci.yml/crates` | ditto | covered |
| `ci.yml/net-e2e` | ditto | covered |
| `ci.yml/sidecar` | ditto | covered |
| `ci.yml/msrv` | ditto | covered |
| `ci.yml/ci-verdict` | guarded, `always()` — **new** | added |

The ticket's last AC — re-confirm `release-sidecar` and `gui-smoke-linux` are still correctly
accepted — is now enforced rather than re-argued: both are `coveredBy` a terminal job, and that
claim is derived from the YAML.

### Guard test

Judged **proportionate and built**, because this is the third instance of the shape in three weeks
(CPE-1872, CPE-1893, CPE-1953) and the previous two were each found by accident.

- `src/lib/ciVerdict.test.ts` — derives from `ci.yml` that `ci-verdict`'s `needs:` is **exactly**
  the set of jobs carrying `needs: lockfile-preflight` (so a sixth job cannot be added and left
  uncovered), that it carries `always()`, and that its step feeds the real script `toJSON(needs)`.
  Then it **executes the job's own `run:` body**, pulled out of the parsed workflow rather than
  retyped, against synthetic payloads.
- `src/lib/catalogPublishLoudFailure.test.ts` — the existing 11-edge ratchet gains a `coveredBy`
  field and a **derived** assertion: every accepted silent skip must name a terminal job that
  exists, carries `always()`/`!cancelled()`, **and genuinely lists the skipped job in its own
  `needs:`**. Clause three is the load-bearing one — a terminal job that does not need the skipped
  job never sees it, so naming it as cover would read as reassurance while being false (CPE-1933).

### Red-proof

**Demonstrated locally (all observed, none inferred):**

1. `ci-verdict`'s real `run:` body, spawned with the exact payload GitHub hands it when
   `lockfile-preflight` fails (all five `skipped`) → **exit 1**, `::error::` naming all five and the
   word `SKIPPED`. Both directions proven: all-five-`success` → **exit 0**.
2. Missing `CI_VERDICT_NEEDS`, malformed JSON, `{}`, a single-job payload, a `null`/array payload,
   and a job with no `result` field → all **exit 1 / not ok**. A gate that cannot see its inputs
   must not report success.
3. Three sabotages of the wiring guard, each reverted:
   - drop `msrv` from `ci-verdict`'s `needs:` → red;
   - `if: always()` → `if: success()` → red;
   - add `needs: lockfile-preflight` to `ffmpeg-pin-guard` (the "sixth job someone forgets" case) → red.
4. Two sabotages of the `coveredBy` derivation, each reverted:
   - point a cover at a job with no `if:` → red ("it would be skipped by the same upstream failure");
   - remove a `coveredBy` → red.

**NOT verified, stated plainly:** no real GitHub Actions run with a genuinely failing
`lockfile-preflight` was triggered. Forcing that means committing a deliberately stale `Cargo.lock`,
which is a poor thing to put in a PR. The claim "the five get skipped" is GitHub's documented
`needs:` semantics plus the measurement already recorded in CPE-1953 (23 consecutive `release.yml`
runs where `run=failure` implied `catalog=skipped`); the claim "`ci-verdict` then reds" is
demonstrated by executing the shipped `run:` body against exactly that payload. The one link untested
end to end is GitHub's own delivery of `toJSON(needs)` to the step, which is a platform behaviour,
not this diff's.

### Checks

`npm run check` clean. `npm test` run. No Rust touched, so no clippy leg needed.
