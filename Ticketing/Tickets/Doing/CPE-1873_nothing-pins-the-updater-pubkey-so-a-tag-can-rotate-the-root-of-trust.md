---
id: CPE-1873
title: nothing pins the updater pubkey, so anyone who can push a tag can rotate the app's root of trust and the release guard will bless it
type: bug
priority: High
status: Doing
tags: ready
estimate: M
created: 2026-08-23
closed:
---

## Problem

`verify-release-artifacts` (CPE-1058) reads the expected updater public key from
`plugins.updater.pubkey` in `src-tauri/tauri.conf.json` — **the same commit that produces the
artifact and the same commit that defines the workflow.** Nothing pins that value.

Found by the independent Security Auditor while auditing CPE-1872, and **demonstrated**, not
inferred. Scenario 10 of its fixture run: swap `tauri.conf.json`'s pubkey to an attacker-generated
key, sign the manifest and the artifact with that same key, run the guard.

```
verify-release-artifacts (CPE-1058)
  manifest   : latest.json
  verifying  : cpe_1.2.3_x64-setup.nsis.zip
OK: manifest + 1 platform signature(s) verified against the configured pubkey.
EXIT=0
```

The auditor searched the repo: `ci.yml` has no pubkey assertion (its only textual hit, at line 860,
is a comment), and there is no guard test or golden value anywhere in the tree.

## What the check does and does not prove

- **Proves:** the manifest's signatures are internally consistent with the pubkey baked into *this*
  build. That catches a botched signing run, a version-bump mismatch, a corrupted artifact, a
  malformed manifest. Genuinely useful, and it is what CPE-1058 was scoped to do.
- **Does not prove:** authenticity against the key **users already trust**. Anyone who can push a
  version tag ships an app whose baked-in updater root of trust is theirs, and the guard reports
  success over it.

Note the attacker does not even need `TAURI_SIGNING_PRIVATE_KEY`: the workflow file itself comes
from the tagged commit, so it can be pointed at attacker-supplied key material.

## Why High

The updater is the highest-trust surface in the product — a compromise here is arbitrary code
executed on every user's machine, silently, with the app's own privileges. The guard that exists to
protect it currently attests to a property (internal consistency) that reads, in its own success
message, like a stronger one (authenticity). A check whose output overstates what it verified is
this repo's most-repeated defect shape, and this is its highest-stakes instance.

Note the exposure is bounded by who can push a tag to this repo — today, a very small set. This is
defence-in-depth against a compromised token or a mistaken rotation, not a live exploit by an
outside party.

## What to do

1. **Pin the expected pubkey in a second, reviewed location** so a commit that rotates it must change
   two places. A CI guard test asserting `tauri.conf.json`'s pubkey equals a literal constant is the
   cheap form.
2. **Better: source the expected pubkey from outside the tree** — a repo secret or organisation
   variable passed into the verify step — so the tagged commit cannot supply both the key and the
   thing it validates.
3. **Make the success message honest** about which property it checked. `OK: manifest + N platform
   signature(s) verified against the configured pubkey` should not be readable as "verified as
   authentic".
4. Decide, and record in the work log, what the intended rotation procedure *is* — a pin is only
   correct if there is a deliberate way through it.

## Acceptance criteria

- [x] A commit that changes `plugins.updater.pubkey` alone fails CI.
- [x] The failure message says plainly that the updater root of trust changed and what to do about it.
- [x] The intended key-rotation procedure is documented where the next person will find it.
- [x] Demonstrated red: the auditor's scenario 10 (self-consistent attacker keypair) now fails.

## Notes

Found alongside three other findings against CPE-1872's PR (#1008), which are being fixed in that
ticket: a manifest platform whose artifact is absent locally is skipped and still counted as
verified; artifact lookup binds to a URL *basename* with first-wins directory order; and the
repo-root `latest.json` the verify step now reads is not gitignored. This ticket is the one finding
that is **not** CPE-1872's to fix — it predates that PR and is a different trust question.

## Work Log

- **2026-08-23 13:40 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from the independent Security Auditor's report on PR #1008. Evidence is the auditor's own
  constructed fixture run, reproduced above verbatim.
- **2026-08-26 (Worker)** — Picked up. Plan: pin the real `src-tauri/tauri.conf.json`
  `plugins.updater.pubkey` value as a literal constant in a second, reviewed file inside
  `crates/updater-verify` (the crate that already builds/tests on every push via
  `ci.yml`'s "updater-verify — clippy + test" step), add a guard test that reads the live
  config and fails loudly + informatively the moment the two disagree, make the
  `verify-release-artifacts` success message state plainly what property it checked
  (internal consistency, not authenticity against a value outside the tagged commit), and
  document the intended rotation procedure in README.md's existing "Auto-updates" setup
  section plus the guard's own doc comment. Will red-proof by committing, then rotating the
  live pubkey to a fresh keypair, running the guard, recording the RED output, and
  restoring.
- **2026-08-26 (Worker)** — Implemented + red-proofed.

  **What shipped.**
  - `crates/updater-verify/src/pinned_pubkey.rs` — `EXPECTED_TAURI_UPDATER_PUBKEY`, a literal copy of
    `src-tauri/tauri.conf.json`'s `plugins.updater.pubkey`, committed in a second, reviewed location
    (module doc carries the full threat framing + the rotation procedure).
  - `crates/updater-verify/tests/pinned_pubkey_guard.rs` — `live_pubkey_matches_the_pinned_copy`, a
    plain `cargo test` that reads the real `src-tauri/tauri.conf.json` and asserts equality against the
    pin. Runs on every push/PR to `main` via `ci.yml`'s existing "updater-verify — clippy + test" step
    (not gated on a tag), so a commit that changes only the pubkey fails CI immediately — satisfies
    acceptance criterion 1.
  - `crates/updater-verify/src/bin/verify-release-artifacts.rs` — the `OK:` success message now states
    plainly that it verified internal consistency with *this checkout's* pubkey, not authenticity
    against the value users already trust, and points at `pinned_pubkey.rs` for the check that does —
    satisfies acceptance criterion 2.
  - `README.md` "Auto-updates — one-time setup" — documents the two-location pin and the rotation
    procedure where a maintainer setting up signing will find it — satisfies acceptance criterion 3.

  **Rotation story (decided + recorded, per step 4 of the ticket):** a legitimate rotation is one PR
  that updates BOTH `tauri.conf.json`'s `pubkey` and `EXPECTED_TAURI_UPDATER_PUBKEY` together, states
  the reason, and separately rotates the `TAURI_SIGNING_PRIVATE_KEY*` GitHub secrets (held outside the
  repo, by whoever generated the new keypair). This does not stop someone with full push access from
  editing both files in the same commit — nothing living inside the repo can, and the ticket's own "Why
  High" section scopes the threat to a compromised token / a careless mistake, not an unrestricted
  malicious insider. What it buys: a one-line, easy-to-miss `tauri.conf.json` diff becomes a two-file
  diff a normal PR review actually surfaces, and an unintended change now fails CI loudly instead of
  quietly reporting success.

  **Red-proof (acceptance criterion 4).** Committed the guard first (green: `live_pubkey_matches_the_pinned_copy ... ok`).
  Then generated a fresh, self-consistent attacker minisign keypair (via a throwaway
  `cargo run --example`, not committed), swapped `src-tauri/tauri.conf.json`'s `pubkey` to the
  attacker's public key — the exact shape of the auditor's scenario 10 — and re-ran the guard:

  ```
  running 1 test
  test live_pubkey_matches_the_pinned_copy ... FAILED

  thread 'live_pubkey_matches_the_pinned_copy' panicked at tests\pinned_pubkey_guard.rs:38:5:
  assertion `left == right` failed:

  SECURITY (CPE-1873): the updater's root-of-trust public key changed.
  `src-tauri/tauri.conf.json` -> plugins.updater.pubkey no longer matches the pinned copy in
  crates/updater-verify/src/pinned_pubkey.rs::EXPECTED_TAURI_UPDATER_PUBKEY.
  ...
    left: "...DBD17E37AC7446C1..." (attacker key)
   right: "...521E574F68E2561A..." (real key)

  test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
  ```

  Exit code 101 — a hard CI failure, not a warning. Restored the real pubkey immediately after
  (`git status --short` confirmed a clean working tree — `src-tauri/tauri.conf.json` back to its
  original bytes), re-ran the guard, confirmed GREEN again, and deleted the throwaway keygen example.
  Note `verify-release-artifacts` itself still reports `OK` for a self-consistent attacker keypair per
  its own now-honest wording (it only ever proved internal consistency, never authenticity) — that is
  expected and unchanged; the pin in this ticket is the independent check that catches what that binary
  by design cannot.

  **Verification.** `cargo clippy --all-targets -- -D warnings` clean. Full crate `cargo test`: 22 lib +
  1 new pinned-pubkey + 15 release_guard = 38/38 pass.
