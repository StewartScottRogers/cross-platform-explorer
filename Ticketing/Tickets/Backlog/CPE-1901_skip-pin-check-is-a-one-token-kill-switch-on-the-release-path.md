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
