---
id: CPE-1901
title: --skip-pin-check is a one-token kill switch for the updater pin, and the tag path's pin is step-gated on a secret
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

Two related weaknesses in how CPE-1873's updater pin is *invoked* on the release path. Neither is a
flaw in the pin itself — both are ways to make it not run.

**1. `--skip-pin-check` disables the whole thing in six words.** Demonstrated: the same
`verify-release-artifacts` invocation against the same tampered config gives `SECURITY (CPE-1873)…`
and exit 1 without the flag, and passes straight through to the manifest read with it.

Adding `--skip-pin-check` to `release.yml`'s existing `cargo run` line turns off the entire round-2
fix on the tag path. That diff reads far more innocuous than a pubkey change — which is precisely the
problem, because the whole point of the pin is that a reviewer should not have to notice a subtle edit
for the root of trust to stay protected. A kill switch reachable from the workflow file recreates the
reviewer-must-notice failure mode one level up.

**2. The tag path's pin is step-gated on a secret being present.** It rides in a step gated
`if: steps.sig.outputs.has == 'true'`. With `TAURI_SIGNING_PRIVATE_KEY` absent, the job concludes
`success` having checked nothing — and the publish gate in `run.md` accepts `success`.

**Practical exposure is bounded**, and this is worth stating rather than alarming over: with no signing
key, tauri-action emits no `.sig` files and no updater manifest at all. `release.yml`'s own comment
(L127-128) calls this "fail-closed — no manifest, nothing to verify", which is correct. The concern is
that a job reporting `success` for "I verified nothing" is indistinguishable, to every downstream
consumer, from one reporting `success` for "I verified and it was fine".

## Acceptance criteria

- [ ] Make `--skip-pin-check` unreachable from a workflow. Gate it behind a `#[cfg(test)]`-only path,
      or an environment variable only the test fixtures set. If the flag exists for a real operational
      reason, record what that reason is — and if there isn't one, delete it.
- [ ] Red-proof: confirm the flag no longer disables the pin when passed the way `release.yml` would
      pass it, and that whatever the fixtures need still works.
- [ ] Make "verified nothing" distinguishable from "verified, all good" in the job's outcome. A
      neutral/skipped conclusion, or an explicit line in the summary, so a `success` never silently
      means the check did not run. Decide which and record why.
- [ ] Check the same shape elsewhere: any other guard in this repo that is step-gated on a secret or
      an optional input, and reports `success` when it skips. List what you find even if you only fix
      this one.

## Notes

Filed 2026-08-26 by CPE-1873's independent Security Auditor, as a note alongside a proven bypass rather
than a merge block. Related: **CPE-1873** (the pin), **CPE-1900** (`CONFIG_CHAIN` drift), **CPE-1874**
(releases that shipped without their signatures verified), **CPE-1872** (the redesigned
`verify-published-manifest` job, which has never actually executed).

Worth reading together with CPE-1874's correction note: `release.yml` has failed at its verify step on
every tagged run since 2026-07-26, and `release-sidecar.yml` — the channel that actually ships — never
had signature verification at all. A kill switch matters more, not less, on a path whose verification
history is already this thin.

## Bound on the kill switch, measured 2026-08-26

CPE-1873's re-reviewer scoped item 1 more tightly than the audit note did, and the bound is worth
recording so whoever picks this up starts from the true state:

`grep -rn "skip-pin-check"` across all `.rs` / `.yml` / `.yaml` files finds **7 occurrences, all of
them inside `crates/updater-verify/tests/release_guard.rs`'s own fixture call sites**. The flag is
**never passed in any workflow** today. So this is a latent hazard — a switch sitting within easy
reach of a one-line workflow edit — not a currently-disabled guard. Priority stays Medium on that
basis; it would be High if a workflow were already passing it.

## A third, related doc overclaim (same release-path invocation)

`crates/updater-verify/src/bin/verify-release-artifacts.rs`'s **top-of-file doc comment** says a bad
manifest "fails the release before it ships". Per the structural finding below, that is not what
happens on the plain channel: `verify-published-manifest` is `needs: release` with
`if: ${{ !cancelled() }}`, so the `release` job has already built, signed and uploaded installers plus
`latest.json` into the draft before it runs.

This predates CPE-1873 — it came in with CPE-1872 (`f97aef8a`, PR #1008) and CPE-1873 did not touch
it. Correct it alongside this ticket's work, since both are about what the release-path invocation
actually guarantees.

## The structural fact both of the above sit on

- `release-sidecar.yml` — **preventive**. `verify-updater-pin` has no job-level `if:`, and
  `release-sidecar: needs: [create-release, verify-updater-pin]` has no override, so a failing pin
  skips the job that builds, signs and publishes. This is the channel `/run` actually installs, so the
  stronger protection covers the real distribution path.
- `release.yml` — **detective**. The pin fails loudly, but only stops a draft becoming public if
  whoever runs `/run` follows `run.md` step 1b-ii, which checks the job's `conclusion == "success"`
  before `gh release edit --draft=false`. Nothing in the CI graph itself prevents that command being
  run by hand.

Also empirically: the new `verify-published-manifest` design merged 2026-08-23 20:27, and the most
recent `release.yml` run is from 2026-08-23 14:36 — about six hours earlier. **No tag push has ever
occurred against the new job graph.** The 20 consecutive `release.yml` failures on record all failed
at the OLD per-matrix step (`verify-release-artifacts: no latest.json found under
../../src-tauri/target`, run `32645968177`, job `97210161557`), which CPE-1872 replaced. So the pin
does not inherit a dead job — but nothing in this flow has run against a real signed, published draft
yet, and that is worth knowing before anyone treats it as proven.
