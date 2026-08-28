---
id: CPE-1978
title: `release.yml`'s "Verify the signed bundle" step checks a `.sig` is **present**, not that it **verifies** — and its own comment names CPE-1954 as the enabler
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by CPE-1954's worker (PR #1088) while routing `catalog-sign verify` through `VerifiedIndex`.

`release.yml`'s **"Verify the signed bundle"** step checks only that a `.sig` file **exists**. It does
not run the verifier. **A step named "Verify" that asserts presence is the shape this repo has spent a
day burning down** — a name that reads as a guarantee over a check that is not the guarantee.

The step's **own comment names CPE-1954 as the enabler**, i.e. it was written knowing the real check
was unavailable and expecting someone to come back. **CPE-1954 has now landed**, so it is available.

## Why the worker deliberately left it

Stated plainly, and correctly: wiring the real verification needs **the public key in CI**, and it is
**release plumbing with its own blast radius**. Folding a secrets change and a release-workflow change
into a PR about a signing tool's read path would have been the scope creep this repo keeps refusing.

## What "verify" should mean here, now that it can

CPE-1954 established what `catalog-sign verify` actually checks, by **running the real binary** rather
than reading names:

| check | now enforced |
|---|---|
| index signature under the supplied key (`verify_index`) | yes |
| per-manifest signature (`trust::verify_signature`) | yes |
| sha256 content binding (`CatalogEntry::matches`) | yes |
| supported index schema (`CatalogIndex::is_supported`) | **added by CPE-1954** |
| `entry.id` is a single safe path component (`is_valid_entry_id`) | **added by CPE-1954** |

**Two things this subsystem does NOT have, so do not go looking for them** (the Foreman's brief for
CPE-1954 wrongly implied both, and the worker corrected it): there is **no filename binding** — these
are raw detached ed25519 signatures over exact bytes, with no trusted comment and no `file:<name>`;
that is the **updater's** minisign scheme, a **different subsystem**. And there is **no version floor**
in `verify` — anti-rollback lives in `apply_bundle_with`, and `verify` applies nothing.

## Acceptance criteria

- [ ] **Demonstrate the gap first.** Put a bundle in the release step's path whose signature does **not**
      verify, and show the step **passing**. Assert on the step's exit status, not on its log text.
- [ ] **Decide how the pubkey reaches CI**, and say what it costs. It is a **public** key, so committing
      it in-repo is defensible and avoids a secret — argue that against a repository secret rather than
      defaulting. Whatever you choose: **never commit a signing (private) key**, and do not touch the
      updater `pubkey`/`endpoints` in `tauri.conf.json`.
- [ ] **The step must fail closed.** A missing binary, an unreadable key, a bundle that is absent, or a
      verifier that cannot run must **red the release**, never pass. CLAUDE.md's rule — *distinguish
      "ran and found nothing" from "did not run"* — and this repo shipped **eight** violations of it in
      one day, one of them inside the guard written to prevent it.
- [ ] **Check whether `release-sidecar.yml` carries the same step**, and any other workflow that
      publishes a bundle. Enumerate at run time (CPE-1932) — the sibling workflow has diverged from
      `release.yml` before.
- [ ] **Red-proof it in CI, not only locally.** A workflow change nobody triggered is untested by
      construction (CPE-1933). If a full release run is unaffordable, say exactly how far you got and
      what remains unverified — **do not describe intended behaviour as observed**.
- [ ] Consider whether `crates/updater-verify`'s existing pattern applies: it **reads a workflow's argv
      at run time and executes the real binary with it**, which is this repo's answer to "a comment that
      claims what a workflow does." `release_workflow_wiring.rs` is the worked example.

## Work Log

**2026-08-28 — worked. What was MEASURED, and what was not.**

*The gap, on exit status.* The pre-fix `Verify the signed bundle before uploading it` body was run
against `catalog-out/catalog-index.json` + a `catalog-index.json.sig` holding the ASCII text
`not a signature`. It printed `signed catalog bundle carries 1 entr(y|ies); files to upload:` and
exited **0** — the job would have uploaded it. That is now a permanent executed case in
`src/lib/catalogPublishLoudFailure.test.ts` §8, asserting `r.status === 0`, not a log string.

*The same bundle through the real binary.* `catalog-sign verify catalog-out <trusted pubkey>` →
`FAIL: index signature does not verify under the key`, exit **1**.

*The new step, real `cargo`, real `catalog-sign`, body extracted from `release.yml` itself.* Four
scenarios, run locally against a bundle signed with a throwaway keypair (deleted):
A. good bundle + its key → exit **0** (`OK: index + 1 manifest(s) verify`, then the control run
   refused under the decoy).
B. same bundle, index `.sig` overwritten with `not a signature` → exit **1**.
C. good bundle, a key that did not sign it (a rotation nobody mirrored) → exit **1**.
D. `cargo` absent from `PATH` → exit **1** (`cargo: command not found`, then the step's `::error::`).

*The pubkey decision.* The key is a **literal in `release.yml`**, not a repository secret. It is the
public half; the identical value is already committed as `CATALOG_TRUSTED_KEYS` in
`src-tauri/src/lib.rs` and ships inside every installed binary, so a secret buys no confidentiality
and costs two things: a second copy no diff and no guard can see (a rotation could silently diverge
from what clients trust — the exact failure this check exists to catch), and an unset secret
expanding to the empty string, i.e. failing **open**. `catalogPublishLoudFailure.test.ts` §8 derives
both sides and reds on any divergence; red-proofed in both directions (workflow literal changed →
red; Rust const changed → red; both reverted). No signing key was generated in-repo, committed, or
touched, and `tauri.conf.json` is unmodified.

*Verifying under the CLIENTS' key, not the signing key,* is deliberate: verifying under the key that
just signed would only prove the bundle is self-consistent.

*Fail-closed, including "did not run".* `set -euo pipefail` covers a missing cargo, a build failure
and an unreadable bundle. The case an exit code cannot cover — a verifier that says yes to
everything — is covered by running the check **twice**, the second time under a key that did not
sign the bundle, requiring a refusal. Executed: a stub `cargo` that approves everything makes the
step exit non-zero.

*Enumeration (CPE-1932).* Derived from `allShellUnits()` over every workflow and extracted script,
not from a remembered pair of filenames. **`release-sidecar.yml` has no catalog job and signs
nothing** — nothing there to diverge. The sign-family invocations on this revision are
`release.yml → catalog-sign` (sign, and now verify) and **`model-snapshot.yml → model-snapshot-sign`
(sign only, no verification before publishing)**.

*Sibling gap found and NOT closed here.* `model-snapshot.yml` signs `models-index.json` with the same
key and publishes it to the `model-catalog` release with no signature check at all — the identical
shape. It is not closed in this PR because `model-snapshot-sign` **has no `verify` subcommand**
(derived, comments stripped: the only `verify` in `model_snapshot_sign.rs` is inside a comment).
Adding one is its own change to a scheduled workflow and wants its own ticket. The guard records this
as a derived fact: the day that subcommand appears, the test reds and starts demanding the wiring.

*What remains UNVERIFIED.* No release was cut and no workflow run was triggered, so the shipped step
has **never executed on a GitHub runner**. What was executed is the step's own `run:` body, extracted
from `release.yml` at run time, under bash — locally with the real `catalog-sign`, and in CI (vitest)
with a key-sensitive stub `cargo`. The one link that stays inferred is that the CI runner's
`CPE_CATALOG_SIGNING_KEY` secret is the private half of `CATALOG_TRUSTED_KEYS`; if it is not, the new
step fails the next release **loudly** — which is the intended behaviour, but it is a prediction, not
a measurement.

## Notes

Filed 2026-08-28 by the sprint Foreman from CPE-1954's worker (PR #1088), which found it while
enumerating the readers of `catalog-index.json` and left it deliberately, with its reason.

Related: **CPE-1954** (PR #1088 — the verifier this unblocks, and the definitive list of what it
checks), **CPE-1940** (`VerifiedIndex`, the fail-closed baseline), **CPE-1951** (the catalog's monotonic
version bound — the other half of release-time catalog correctness), **CPE-1933** (a claim about a
workflow that nobody executes is untested by construction), **CPE-1932** (enumerate, don't recall).
