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

- **2026-08-26 (Worker) — attempt 2, PR #1028 came back CHANGES REQUESTED.** Three independent
  reviewers ran in parallel on attempt 1 and all three findings were correct; none argued, all fixed.

  **Finding 1 (independent reviewer, BLOCKING) — the guard never ran on any path that produces a
  release.** `ci.yml` has no `tags:` trigger, so `pinned_pubkey_guard.rs`'s `#[test]` — which only runs
  via `cargo test -p cpe-updater-verify` — never executes for a tag push. `release.yml`'s
  `verify-published-manifest` job runs `verify-release-artifacts` (a binary, not the test), which
  attempt 1 never wired to the pin at all — only its `println!` changed. `release-sidecar.yml`
  (`workflow_dispatch` only, the channel `/run` actually installs) had zero references to
  `updater-verify`/`pinned_pubkey`/`cargo test` anywhere. Net: a tag pointed at any commit — including
  one that never touched `main`, or one CI left red — built and signed under both release workflows
  without the guard ever running. Branch protection confirmed absent (`branches/main/protection` 404,
  `rulesets` []), so nothing else closed this gap either.

  **Finding 2 (independent Security Auditor, BLOCKING, DEMONSTRATED) — the pin only ever read the BASE
  config; the shipped sidecar channel is a merged overlay chain.** The auditor added an updater
  override block to `src-tauri/tauri.sidecar.conf.json` alone (base `tauri.conf.json` untouched), ran
  the full crate suite, and got 38/38 green — including the base-config pin — while the actual channel
  `release-sidecar.yml` ships (base config + `CONFIG_CHAIN` overlays, per
  `src/lib/sidecarBundleResources.test.ts`) had an attacker-controlled `plugins.updater.pubkey` AND
  `.endpoints`. Corroborated in-repo: `tauri-utils` merges configs with `json_patch::merge` (RFC 7386),
  the same mechanism CPE-1270/1271 already documented overriding `bundle.resources`.

  **Finding 3 (independent UAT tester, non-blocking, confirmed the mechanism itself is sound) —**
  the guard's message quality, formatting robustness (JSON reformat + key reordering stays green,
  matters because CPE-1850 is open about a lossy reserialize of this exact file), and real-CI operation
  (quoted `test live_pubkey_matches_the_pinned_copy ... ok` at line 17372 of the 17,533-line raw log for
  job `98190947856`, SHA `3818dbd3`) were all judged exemplary and NOT changed. One defect: the rotation
  procedure's step 1, `npm run tauri signer generate -- -w ./updater.key`, is interactive and panics
  with no TTY (`Os { code: 233, ... "No process is on the other end of the pipe." }`) — pre-existing in
  the base setup docs, but now load-bearing because this ticket's guard sends a maintainer there under
  pressure. Fixed in both `pinned_pubkey.rs` and README with the `--ci`/`-p` non-interactive forms.

  **What shipped, round 2:**
  - `crates/updater-verify/src/bin/verify-release-artifacts.rs` — the pin (pubkey AND now `endpoints`,
    to block a downgrade attack: repointing where the app fetches `latest.json` from can serve an
    older, genuinely-signed, vulnerable build forever even with the pubkey pin intact) is checked
    directly inside this binary, before it looks at any manifest/artifact. This is what
    `release.yml`'s `verify-published-manifest` job already runs on every `v*` tag push — the release
    path is now covered with no `release.yml` edit needed. New `--skip-pin-check` flag (opt-OUT,
    default off) exists only for this crate's own fixtures, which use fresh throwaway keypairs per
    test unrelated to the real pin; `tests/release_guard.rs`'s 7 call sites now pass it explicitly,
    with a comment explaining why.
  - `.github/workflows/release-sidecar.yml` — new `verify-updater-pin` job, required via `needs:`
    before `release-sidecar` (the actual build/sign/publish matrix) can run: `cargo test --test
    pinned_pubkey_guard` (base config) + `npx vitest run src/lib/sidecarBundleResources.test.ts`
    (merged overlay chain). A rotated-without-updating-the-pin config, base OR overlay, now blocks this
    channel before it ever signs anything.
  - `src/lib/sidecarBundleResources.test.ts` — new `describe` block, 6 assertions (pubkey + endpoints ×
    windows/linux/macos), built on the file's existing `CONFIG_CHAIN`/`mergeJson`/`loadConfig` machinery
    (refactored `mergedBundleResources` to share a new `mergedConfig(os)` helper). Failure message names
    the exact overlay chain and states plainly this IS the shipped channel's real root of trust.
  - Three overclaiming strings corrected exactly as flagged: `pinned_pubkey_guard.rs`'s module doc now
    states precisely where each check runs and where it doesn't (no `tags:` trigger, no overlay
    awareness) instead of the false "fails CI before it ever reaches a release tag" claim;
    `pinned_pubkey.rs`'s "impossible to satisfy by touching only one file" is now qualified to the
    paths where that's actually true, plus a new "What none of this proves" section stating the ceiling
    outright; `verify-release-artifacts`'s `OK:` message no longer claims to point at an "authenticity"
    check — it says self-consistency, "second in-repo pin," full stop.
  - `README.md` — non-interactive keygen invocation (`--ci` / `-p`) documented alongside the
    interactive one; rotation section rewritten for three pin locations plus the same honest ceiling.

  **Red-proof, all three paths, done for real:**
  1. *Release path, pubkey* — rotated `src-tauri/tauri.conf.json`'s live pubkey to a fresh throwaway
     keypair (generated via a temporary, uncommitted `cargo run --example`, deleted after), then ran
     the EXACT command `release.yml:295-299` runs on a tag push:
     `cargo run --manifest-path crates/updater-verify/Cargo.toml --bin verify-release-artifacts -- --conf src-tauri/tauri.conf.json --manifest release-assets/latest.json --search release-assets --expect-url-prefix "https://github.com/.../releases/download/v0.57.69/"`
     — got `SECURITY (CPE-1873): the updater root of trust changed. ... configured: ...B30DAC63...
     pinned: ...521E574F...`, exit code 1. Restored via `git checkout -- src-tauri/tauri.conf.json`,
     confirmed `git status --short` clean.
  2. *Release path, endpoints* — same file, changed `plugins.updater.endpoints` to
     `["https://attacker.example/latest.json"]`, ran the identical command — got `SECURITY (CPE-1873):
     the updater's manifest endpoint(s) changed. ... configured: ["https://attacker.example/..."]
     pinned: ["https://github.com/.../latest.json"]`, exit code 1. Restored, confirmed clean, re-ran
     `cargo test --test pinned_pubkey_guard` green.
  3. *Merged overlay chain* — added the auditor's exact override block (attacker pubkey + endpoint) to
     `src-tauri/tauri.sidecar.conf.json` ALONE, base file untouched. `npx vitest run
     src/lib/sidecarBundleResources.test.ts`: **6 of 10 tests failed** (pubkey + endpoints, all three
     OSes), each naming the overlay chain
     `tauri.conf.json -> tauri.sidecar.conf.json -> tauri.sidecar.unix.conf.json -> ...`; the 4
     pre-existing CPE-1271 resource tests stayed green (unaffected). In the SAME state, ran the full
     Rust crate suite: **39/39 still passed** — reproducing, exactly, the auditor's demonstrated
     bypass shape (base-config guard blind to an overlay-only change) before restoring. `git checkout
     -- src-tauri/tauri.sidecar.conf.json`, confirmed clean, re-ran vitest: 10/10 green.

  **What this still does NOT prove, stated for the record (per the Security Auditor's requirement):**
  every guard added in both rounds compares files read from the SAME commit/checkout — the base config
  against an in-crate constant, the merged overlay chain against an in-repo TS constant, the release
  binary's live read against its own compiled-in constant. A rotation that updates the config (base or
  overlay) and every pin together in one commit is perfectly self-consistent and passes all of them.
  Continuity with the key **users already trust** across real releases is not checked by CI anywhere;
  it was established for this repo only by the Security Auditor going and measuring it externally —
  fetching the live v0.57.69 sidecar installer and verifying its signature under the pinned key (exit
  0), and reading `plugins.updater.pubkey` at tags v0.1.0, v0.2.0, v0.10.0, and v0.57.69 (same key at
  every one). Nothing in this PR performs that check automatically.

  **What would actually close that gap (recommended for the next pickup, NOT built here — out of
  scope for this round, bigger change):** the ticket's own option 2 — source the expected pubkey from
  OUTSIDE the tree, e.g. a GitHub Actions repository/organisation **variable** (public value, no
  secret needed) read by `verify-published-manifest` and the new `verify-updater-pin` job, compared
  against `tauri.conf.json`'s live value the same way the in-repo constant is today. The key property
  that would add: the tagged commit could no longer supply BOTH the value under test and the value it's
  checked against — an attacker with commit access could still change `tauri.conf.json`, but could not
  also change what CI compares it to without separate access to the repo/org settings. That is a
  genuinely different trust boundary than anything in this PR, which is why it's flagged for a future
  ticket rather than folded in here.

  **Verification, round 2.** `crates/updater-verify`: `cargo clippy --all-targets -- -D warnings`
  clean; `cargo test` 39/39 (22 lib + 2 pinned-pubkey [pubkey + endpoints] + 15 release_guard, all
  updated to pass `--skip-pin-check`). Frontend: `npx vitest run` (full suite) 331 files / 4463 tests
  pass; `npm run check` (svelte-check) 0 errors, 0 warnings. Both workflow YAMLs
  (`release-sidecar.yml`, `release.yml`) parse clean under `yaml.safe_load`.

- **2026-08-26 (Worker) — attempt 3, PR #1028 (SHA `653fa838`) re-reviewed. Four of five original
  findings CLOSED by execution (the auditor re-ran every attempt-1/2 attack and confirmed it stays
  red/covered — overlay bypass, endpoints pin, `release-sidecar.yml`'s real gate, the tag path firing).
  One NEW finding — finding 6 — is why this attempt exists.**

  **Finding 6 (independent Security Auditor, BLOCKING, DEMONSTRATED) — a bypass that doesn't go
  through `--config` at all.** Tauri's own `tauri-utils::config::parse::read_from` reads
  `tauri.conf.json` and then AUTOMATICALLY looks for `tauri.macos.conf.json` /
  `tauri.linux.conf.json` / `tauri.windows.conf.json` next to it and merges each via RFC 7396 — no
  `--config` flag, no workflow involvement, unconditional on every build. None of the three files
  exists in this repo and none was in `CONFIG_CHAIN` (which only lists overlays `release-sidecar.yml`
  explicitly passes via `--config`). The auditor's proof: `src-tauri/tauri.windows.conf.json`
  containing only `{ "plugins": { "updater": { "pubkey": "...", "endpoints": [...] } } }` left
  `sidecarBundleResources.test.ts` 10/10 green, `crates/updater-verify`'s full suite 39/39 green, and
  `verify-release-artifacts` passing the pin check outright (failing only later, on a missing
  manifest path unrelated to trust). Every guard from rounds 1-2 was blind to it, on every Windows
  build, plain channel and sidecar both — and it's better camouflage than the overlay attack, since
  `tauri.windows.conf.json` is a normal, expected Tauri filename that reads as "Windows bundle tweak"
  and can carry a genuine Windows setting right beside the `plugins.updater` block.

  **What shipped, round 3 — the auditor's own recommended fix, an existence-only check (not a
  merge/validate), applied on both sides:**
  - `crates/updater-verify/tests/pinned_pubkey_guard.rs` — new
    `no_automatic_per_platform_config_overrides_the_updater_pin`: for each of the three filenames,
    either it doesn't exist (the expected state today) or, if it does, it must not carry a
    `plugins.updater` key at all. Covers the base/tag path.
  - `src/lib/sidecarBundleResources.test.ts` — matching `describe` block, same three filenames, same
    existence-only check. Covers the sidecar path.
  - Refactored `repo_tauri_conf_path()` in the Rust test file to build on a new shared
    `repo_src_tauri_dir()` helper (needed the directory itself, not just the base config path).

  **Two required doc corrections, both applied, both one clause as scoped:**
  1. `pinned_pubkey.rs`'s and `sidecarBundleResources.test.ts`'s claim that the merged-overlay guard
     computes "the FULL merged config" was false while finding 6 stood (it never saw the automatic
     per-platform files). Corrected to scope the claim to "the full merged `--config` overlay chain"
     with an explicit pointer to the new finding-6 check for the automatic path, plus a new paragraph
     in `pinned_pubkey.rs`'s module doc describing finding 6 and where it's closed.
  2. The PR body's threat model said the guard "fails the tag path before it signs or publishes
     anything." True for `release-sidecar.yml` (its `verify-updater-pin` job gates `release-sidecar`
     via `needs:` before any build/sign/publish step runs), **not** true for `release.yml` — there the
     pin is DETECTIVE, not preventive: `verify-published-manifest` is `needs: release`, and by the
     time it runs the `release` matrix has already built, signed, and uploaded installers plus
     `latest.json` into the DRAFT release. Nothing reaches the public without the separate manual
     publish gate (`/run`'s own asset-presence check), so the end outcome is still correct, but the
     PR body is being corrected to say "detective" for the plain channel, "preventive" for the
     sidecar channel, accurately, rather than claiming prevention for both.

  **Red-proof, all three filenames, both sides, done for real (not just one and assumed three, per
  explicit instruction):**
  - `tauri.windows.conf.json` with the attacker block: vitest — `1 failed | 12 passed`, naming
    `src-tauri/tauri.windows.conf.json exists and sets plugins.updater`; `cargo test --test
    pinned_pubkey_guard` — `2 passed; 1 failed`, `SECURITY (CPE-1873 finding 6): ...tauri.windows.conf.json
    exists and sets plugins.updater`, exit 101. Deleted, confirmed `git status --short` clean.
  - `tauri.macos.conf.json`, identical block: same shape, vitest `1 failed | 12 passed` naming
    `tauri.macos.conf.json`; Rust `SECURITY (CPE-1873 finding 6): ...tauri.macos.conf.json exists...`,
    exit 101. Deleted, confirmed clean.
  - `tauri.linux.conf.json`, identical block: same shape, vitest `1 failed | 12 passed` naming
    `tauri.linux.conf.json`; Rust `SECURITY (CPE-1873 finding 6): ...tauri.linux.conf.json exists...`,
    exit 101. Deleted, confirmed clean.
  - Final state after all three probes: `cargo test` in `crates/updater-verify` 40/40 (22 lib + 3
    pinned-pubkey [pubkey, endpoints, finding-6] + 15 release_guard); `npx vitest run
    src/lib/sidecarBundleResources.test.ts` 13/13; `npm run check` clean; `git status --short` clean
    (no attacker fixture left behind, no private key material ever generated for this round — the
    attacker "pubkey" used is a plain non-minisign placeholder string, since finding 6's check never
    parses it as a key, only checks for the JSON key's presence).

  **Not in scope for this round, filed separately per the Foreman as CPE-1900 / CPE-1901, not
  touched here:** `CONFIG_CHAIN` being a hand-maintained literal with nothing tying it to
  `release-sidecar.yml`'s actual `--config` args; `--skip-pin-check` being a six-word kill switch on
  the tag-path `cargo run` line.

  **Rebased onto `main`** (moved since attempt 2 landed — CPE-1734, CPE-1869, CPE-1889 and others),
  pushed. New head SHA recorded at the top of the final commit for this round.

  **Verification, round 3.** `crates/updater-verify`: `cargo clippy --all-targets -- -D warnings`
  clean; `cargo test` 40/40. Frontend: `npx vitest run src/lib/sidecarBundleResources.test.ts` 13/13;
  `npm run check` (svelte-check) 0 errors, 0 warnings.

  **CI status: GitHub Actions was in a declared major outage during this round** (critical incident,
  database primary failover, runs queued and not starting) — did not wait on or poll CI per explicit
  instruction not to. All verification above is local. CI is genuinely pending on the pushed SHA, not
  quietly green and not quietly broken; report to the Foreman states this explicitly rather than
  claiming a result that wasn't observed.
