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

## Notes

Filed 2026-08-28 by the sprint Foreman from CPE-1954's worker (PR #1088), which found it while
enumerating the readers of `catalog-index.json` and left it deliberately, with its reason.

Related: **CPE-1954** (PR #1088 — the verifier this unblocks, and the definitive list of what it
checks), **CPE-1940** (`VerifiedIndex`, the fail-closed baseline), **CPE-1951** (the catalog's monotonic
version bound — the other half of release-time catalog correctness), **CPE-1933** (a claim about a
workflow that nobody executes is untested by construction), **CPE-1932** (enumerate, don't recall).
