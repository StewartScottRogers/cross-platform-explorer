---
id: CPE-1981
title: '`model-snapshot.yml` publishes a signed `models-index.json` with **no** signature check, and `model-snapshot-sign` has no `verify` subcommand to call'
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by **CPE-1978**'s worker (PR #1095) while enumerating — at run time, via `allShellUnits()` over
every workflow and every script — which workflows publish a signed bundle. It is the **identical shape**
to the defect CPE-1978 closed, one workflow over.

- **`release.yml`** — had a step named *"Verify the signed bundle before uploading it"* that only checked
  the `.sig` **exists and is non-empty**. Closed by CPE-1978: the real `catalog-sign verify` now runs
  before the upload, under the clients' own key.
- **`release-sidecar.yml`** — has **no catalog job and signs nothing**. Nothing to close. (Recorded here
  because the sibling has diverged from `release.yml` before, and "we checked" is worth more written down
  than remembered.)
- **`model-snapshot.yml`** — **signs `models-index.json` with the same key and publishes it to the
  `model-catalog` release with no signature check at all.** Not even the presence check `release.yml` had.

## Why it was left open rather than fixed in PR #1095

Stated plainly by that worker, and correctly: **`model-snapshot-sign` has no `verify` subcommand.** There
is nothing to call. That was **derived, not assumed** — read out of the `[[bin]]` its own
`--manifest-path` declares, with comments stripped first (CPE-1933 rule 2). Adding a subcommand to a
signing tool inside a PR about a *different* workflow's verification step would have been the scope creep
this repo keeps refusing.

**The guard PR #1095 added reds the day a `verify` subcommand appears** — so this is registered in code,
not only in the queue.

## What this needs

- [ ] **Demonstrate the gap first, on exit status, not on log text.** Put a `models-index.json.sig` that
      does not verify into the publish path and show the job **passing**. CPE-1978's worker did exactly
      this for `release.yml` with a `.sig` holding the ASCII text `not a signature` — reuse the shape.
- [ ] **Add a `verify` subcommand to `model-snapshot-sign`.** `catalog-sign verify` is the worked
      example, and CPE-1954 established what it actually checks by **running the real binary**: index
      signature under the supplied key, per-manifest signature, sha256 content binding, supported index
      schema, and `entry.id` as a single safe path component. Decide — and **say** — which of those the
      model index has an analogue for. Do not assume the two index formats are the same shape.
- [ ] **Two things this key's subsystem does NOT have.** There is **no filename binding**: these are raw
      detached ed25519 signatures over exact bytes, no trusted comment, no `file:<name>` — that is the
      **updater's minisign** scheme, a *different subsystem*. And there is **no version floor** in a
      `verify`; anti-rollback lives in the apply path. A brief that implies either has already been
      written once (CPE-1954's) and corrected once.
- [ ] **Verify under the key the CLIENTS trust**, not the signing key — signing-key self-consistency
      proves nothing. CPE-1978 argued this out and put the public half as a literal in the workflow
      rather than a secret, because an **unset secret expands to `""` and fails open**, and a second
      copy no diff can see could silently diverge from `CATALOG_TRUSTED_KEYS`. Follow that decision or
      argue against it explicitly; do not default.
- [ ] **Fail closed on "did not run".** Missing binary, unreadable key, absent bundle, verifier that
      cannot run — every one must **red the job**. And cover the case an exit code cannot: **run the
      check a second time under a key that did not sign the bundle and require a refusal**, so a
      verifier that says yes to everything is caught. This repo shipped **eight** violations of the
      ran/did-not-run distinction in one day, one of them inside the guard written to prevent it.
- [ ] **Red-proof in CI, not only locally** (CPE-1933). A workflow change nobody triggered is untested by
      construction. If a real run is unaffordable, **say exactly how far you got and what remains
      unverified — never describe intended behaviour as observed.** PR #1095's honest list is the model:
      *no release cut, no workflow run triggered, and the runner's private key being the half of the
      committed public key remains inferred.*
- [ ] **Extend the existing guard rather than growing a second scanner.**
      `crates/updater-verify/tests/release_workflow_wiring.rs` reads a workflow's argv at run time and
      **executes the real binary with it**. Anchor on parsed code, never on comment text — PR #1095
      proved this leg by replacing a real invocation with a `#` comment carrying identical text and
      watching **6** assertions red.

## Notes

Filed 2026-08-28 by the sprint Foreman from CPE-1978's worker (PR #1095), which found it by enumerating
rather than recalling, and left it deliberately with its reason.

Related: **CPE-1978** (PR #1095 — the same defect in `release.yml`, and the pubkey-vs-secret argument),
**CPE-1954** (PR #1088 — what `catalog-sign verify` actually checks, established by running it),
**CPE-1940** (`VerifiedIndex`, the fail-closed baseline), **CPE-1951** (the catalog's monotonic version
bound), **CPE-1939** (the model snapshot's `==`/`<` conflation — the *other* known defect in this same
workflow's index handling, worth doing in the same neighbourhood), **CPE-1932** (enumerate, don't
recall), **CPE-1933** (derive provenance, don't claim it).
