---
id: CPE-1923
title: `verify-release-artifacts` passes at exit 0 on three hostile manifests — including a genuinely-signed **downgrade** to an older installer
type: bug
priority: High
status: In Progress
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

An independent Security Auditor built hostile manifests against the updater verification gate and
found **three that pass at exit 0**. All three are pre-existing in
`crates/updater-verify/src/bin/verify-release-artifacts.rs` and `.../src/lib.rs` — none were
introduced by CPE-1908 — but that PR makes this binary the gate for **both** release channels, so
its blind spots now cover everything users install.

Every case was run exactly as `release-sidecar.yml` invokes the binary, with fixtures signed by a
throwaway minisign keypair inside a scratch worktree. No real signing material was used.

For contrast, the gate **correctly rejected**: a mixed plain/sidecar manifest; a sidecar-correct
basename on a foreign host; a signature made over different bytes; empty/absent `platforms`; and a
plain asset with a `#sidecar` URL fragment.

## Finding 1 — signed downgrade (the serious one)

`verify-release-artifacts.rs:352-383` binds the manifest's `version` to `tauri.conf.json`'s
`version`, but **never binds either to the artifact**. Nothing checks that a referenced asset's
basename or bytes belong to the version being shipped.

Exploitation: an actor with only **release-asset write** — a leaked PAT, or any workflow whose
`contents: write` `GITHUB_TOKEN` can be induced to upload; **no signing-key access needed** —
uploads the old, vulnerable `Cross-Platform.Explorer_(Sidecar)_0.1.0_x64-setup.nsis.zip` and its
**genuine** old signature to the new draft tag, and writes a `latest.json` whose `version` is the new
one. Demonstrated: `OK: verified 1 of 1 platform signature(s)`, **EXIT 0**. `latest.json` is itself
unsigned and the Tauri updater compares only the manifest's `version`, so published users
auto-"update" onto the older signed build.

This is the same downgrade outcome CPE-1873's endpoint pin exists to prevent, reached through the
**asset** instead of the endpoint.

**Note when fixing:** a blanket "basename must contain `version`" rule breaks macOS, whose updater
artifact is `<productName>.app.tar.gz` with no version in the name.

## Finding 2 — platform/asset mismatch passes

Same site. A manifest where `darwin-aarch64` serves the sidecar `.nsis.zip` and `windows-x86_64`
serves the sidecar `.app.tar.gz`, each with its own genuine signature: channel purity, URL prefix and
all signatures pass — `verified 2 of 2 platform signature(s)`, **EXIT 0**. Outcome is
denial-of-update (wrong-platform payload) rather than code execution, but the platform→asset mapping
is exactly what a channel-mixing bug corrupts.

Cheap fix: assert each platform key's expected extension set — `windows-*` → `.nsis.zip`/`.msi.zip`,
`darwin-*` → `.app.tar.gz`, `linux-*` → `.AppImage.tar.gz`/`.deb`.

## Finding 3 — channel inference is an unanchored substring match

`crates/updater-verify/src/lib.rs:397-404` decides the channel with
`basename.to_ascii_lowercase().contains("sidecar")`. A plain-channel installer uploaded as
`Cross-Platform.Explorer_1.2.3_x64-setup.nsis.zip.sidecar` reads as `Channel::Sidecar` and passes:
**EXIT 0**, `verifying: Cross-Platform.Explorer_1.2.3_x64-setup.nsis.zip.sidecar`.

So the guard proves "the name contains the word sidecar", not "this asset came from the sidecar
build". Anyone who can name a release asset can flip its apparent channel in **either** direction.

Fix: match against the real `productName` token (`Explorer_(Sidecar)_`), anchored, rather than a free
substring.

## Finding 4 (Low) — vacuous success when the signing secret is absent

`release-sidecar.yml:691`/`:716` (and identically `release.yml:291`/`:328`) gate both real steps on
`steps.sig.outputs.has == 'true'`. With `TAURI_SIGNING_PRIVATE_KEY` unset the job runs, skips both,
and concludes `success` — and `RELEASING.md`'s new publish gate reads that conclusion as proof of
verification. Impact is bounded today (with no signing key the matrix legs fail before producing a
`latest.json`), but **deleting one repo secret silently converts both gates into green no-ops.**

Fix: have the sig-detect step `exit 1` on a tag dispatch instead of emitting `has=false`, or have the
verify step emit a marker the doc check greps for.

## Acceptance criteria

- [x] Bind the artifact to the version for findings 1 — and handle the macOS naming exception rather
      than breaking it.
- [x] Assert the platform-key → extension mapping (finding 2).
- [x] Anchor the channel inference to the real product-name token (finding 3).
- [x] Close the absent-secret vacuous success (finding 4).
- [x] **Land the auditor's hostile manifests as fixtures**, so each fix has something that goes red
      without it. Three of these passed at exit 0 with genuine signatures — reasoning about them is
      not enough.
- [x] Re-run the full hostile set after the fixes and record which now fail and with what message.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1039's independent Security Auditor. Deliberately
scoped **out** of #1039, which fixes the channel-purity coverage gap it was filed for.

Related: **CPE-1908** (the channel-purity guard), **CPE-1873** (the endpoint pin this routes around),
**CPE-1901** (`--skip-pin-check` as a one-token kill switch, plus the unconditional
"matches the second in-repo pin" success line that prints even when the check was skipped),
**CPE-1874** (six shipped releases never signature-checked), **CPE-1917** (plain release broken 27 days).

## Coupling note added 2026-08-27 — read before fixing finding 4

CPE-1908's coverage ratchet (`src/lib/channelPurityCoverage.test.ts`) now **requires** the verify
step's `if:` to be exactly `steps.sig.outputs.has == 'true'`, via a `SIGNING_KEY_STEP_IF` constant.
That is the very condition finding 4 says must change.

So whoever closes finding 4 must update `SIGNING_KEY_STEP_IF` **in the same change**, or the ratchet
goes red on the fix — a guard blocking its own remedy. Flagged by PR #1039's Security Auditor on
re-audit, and a pointer comment was added at the constant.

## Work Log

### 2026-08-27 — implemented (all four findings)

**Ground truth first, and it changed the design twice.** Before writing any rule I read the real
asset names off this repo's own published releases (`gh release view v0.57.69-sidecar`, read-only)
and the real `latest.json` it ships. Two of the ticket's suggested fixes are wrong against reality
and would have broken every release had they been implemented as written:

- The ticket proposes anchoring the channel check on the token `Explorer_(Sidecar)_`. Tauri's
  bundler does not emit that. It emits `Cross-Platform.Explorer.Sidecar._…` for most targets and
  `Cross-Platform.Explorer.Sidecar.-…` for the RPM — two different spellings of the same
  `productName`. A guard anchored on the predicted token would have rejected 100% of real sidecar
  assets.
- The ticket proposes the extension sets `.nsis.zip`/`.msi.zip`, `.AppImage.tar.gz`. This repo ships
  `createUpdaterArtifacts: true` (not `"v1Compatible"`), so its updater payloads are the plain
  `.exe` / `.msi` / `.AppImage` / `.deb` / `.rpm`. The v1Compatible spellings are accepted too, so
  flipping that config setting cannot silently red a release, but they are not what ships today.

Both corrections are pinned by tests that assert against the real published manifest, reproduced
verbatim in `artifact_binding.rs`'s test module, so the next person changing these rules is checked
against what actually shipped rather than what a ticket predicted.

**Finding 1 (signed downgrade) — the anti-rollback decision, in one place.**
New module `crates/updater-verify/src/artifact_binding.rs` owns it:
`platforms_not_bound_to_version(manifest, expected_version)`. Every platform's asset filename must
carry, as a *delimited* token, the version from `--conf` — deliberately the config's version, not
the manifest's own `version` field, because an attacker who can write `latest.json` writes that
field too, so binding to it would prove nothing. Called exactly once, from the binary.

`verify_update_manifest`'s existing `VersionMismatch` is **not** a second copy of this rule and is
documented as such: it compares two fields for self-consistency, says nothing about the bytes, and
passed the auditor's downgrade cleanly. "Signature valid" and "acceptable version to move to" are
now visibly two different questions answered in two different places.

*The macOS exception, handled rather than broken.* `Cross-Platform.Explorer_universal.app.tar.gz`
has no version in it — that is how Tauri names the macOS updater artifact, confirmed on the live
release. The exemption is as narrow as the fact forcing it: the platform key must resolve to
`darwin-*` **and** the basename must end `.app.tar.gz`. A Windows or Linux key cannot claim it by
renaming its payload (tested both directions), and a `darwin-*` key that claims it is simultaneously
held to finding 2's mapping check, which permits `darwin-*` nothing else. **Residual, recorded not
hidden:** a macOS `.app.tar.gz` is still bound to the release only by its url prefix (CPE-1872) and
its signature, never by its own name. Closing that needs reading `CFBundleShortVersionString` out of
the tarball, i.e. tar+gzip dependencies in a crate deliberately kept to one crypto dep — out of
scope here, flagged for a follow-up. The binary now **prints every exemption it grants**, so a run
cannot consist entirely of exemptions and still read as a clean verification.

**Finding 2 (platform/asset mismatch).** `platforms_with_wrong_extension_for_key` asserts each
platform key's payload is one its own OS's bundler produces. An unrecognised OS prefix is a failure,
not a shrug — that is the shape a smuggled entry takes.

**Finding 3 (unanchored channel inference).** `basename.contains("sidecar")` proved only "the name
contains the word", which flipped in *both* directions for anyone who can name a release asset.
Replaced with an anchored comparison against the config's own `productName`, normalised on both
sides (lowercase, alphanumerics only) so it does not depend on guessing Tauri's sanitiser — which is
what makes the RPM's different spelling work. The plain product name is a strict prefix of the
sidecar one, so the plain direction carries one extra clause (`<plain>sidecar`) that keeps CPE-1894's
original catch intact; the sidecar direction needs none. Consequence: `productName` is now
**required** in `--conf`, and a config without one is refused rather than silently disarming the
check.

**Finding 4 (vacuous success when the secret is absent).** Took the ticket's first option — the
sig-detect step now `exit 1`s on a **tag** build with no signing key, instead of emitting
`has=false` and letting the job skip both real steps and conclude `success`. Chosen deliberately
over changing the downstream `if:`: that condition is pinned from two directions
(`releaseVerifyWiringGuard.test.ts` requires the download and verify steps to share one gate, and
CPE-1908's ratchet pins its literal text), so failing *before* the gate closes the finding without
the "guard blocking its own remedy" collision the ticket's coupling note warns about. **No
`SIGNING_KEY_STEP_IF` change was needed, and none was made** — the constant does not exist on `main`
yet (PR #1039 is unmerged), and the condition it pins is untouched.

Scope note: the ticket cites `release-sidecar.yml:691/:716`. Those lines do not exist on `main` —
CPE-1908 has not merged, so `release-sidecar.yml` does not run this binary yet. Finding 4 is fixed in
`release.yml`, the copy that is real in this tree; the sidecar half arrives with #1039 and should
copy this step verbatim.

### Evidence

Each of the three hostile manifests is landed as an end-to-end fixture in
`crates/updater-verify/tests/hostile_manifests.rs` — real throwaway minisign keypair, real
signatures over real bytes on disk, the real binary, invoked the way the release workflow invokes
it. Every assertion is on the **exit status first**; the message assertions come after. No real
signing material is used or touched.

Measured by reverting `src/` to `HEAD` and re-running the same fixtures: **7 of them exit 0 against
the unfixed binary**, including all three headline cases. With the fix, all 17 pass, and the
legitimate-manifest controls (plain channel, sidecar channel, a full five-platform release including
the versionless macOS asset) still exit 0 — a verifier that rejects everything would not be a fix.

Sabotage rounds, each reverted after measuring:

| Sabotage | Red |
|---|---|
| Restore the `contains("sidecar")` substring rule | 3 unit + 2 end-to-end |
| `if false &&` the version-binding refusal | 2 end-to-end, both on the exit code |
| `if false &&` the mapping refusal | 2 end-to-end, one on the exit code |
| Restore the permissive one-line sig-detect step | 1 vitest, on the exit code |

The mapping sabotage initially reddened only on "the refusal names the wrong property" — the version
binding caught its fixture for a different reason. That is defence in depth working, but it meant no
test proved the mapping check itself did anything, so
`h2_only_the_mapping_check_can_refuse_a_wrong_os_payload_that_is_otherwise_perfect` was added: a
correctly-versioned, channel-pure, correctly-prefixed, genuinely-signed macOS payload under a Windows
key, which only the mapping check can refuse.

The finding-4 guard executes the step's `run:` script extracted from the YAML and asserts on the
process exit code and what it wrote to `$GITHUB_OUTPUT`, not on the script's text — a guard asserting
on a string while the process exits 0 is the defect this ticket is about.

### 2026-08-27 (later) — rebased onto merged CPE-1908, sidecar half added

PR #1039 (CPE-1908) merged while this was in review, so deferred item 2 unblocked. Rebased onto
`origin/main` and added the sidecar half. The rebase was **not** mechanical — #1039 changed the
premise of finding 3's fix.

**#1039 broke the anchor, and a verbatim rebase would have shipped that.** CPE-1908 added
`--expect-channel`, which overrides the `productName` derivation, because
`release-sidecar.yml` passes **the plain `src-tauri/tauri.conf.json`** (it needs that file's
pubkey/version and the CPE-1873 pin, which the sidecar overlay never touches) while checking a pure
*sidecar* manifest. So on every real sidecar run, `--conf`'s `productName` and the expected channel
legitimately **disagree**.

My anchored check took its token straight from `productName`. Post-merge that means: expected
channel Sidecar, anchor token `crossplatformexplorer`, and every genuine sidecar asset
(`crossplatformexplorersidecar…`) failing to match — the *same* "rejects 100% of real sidecar
assets" outcome I caught in the ticket's own proposed token, arriving by a different route. The
conflict resolution therefore split the two roles apart:

- `base_product_token()` reduces whatever config was passed to the channel-free **base identity**
  (both `Cross-Platform Explorer` and `Cross-Platform Explorer (Sidecar)` yield the same base).
- `channel_product_token(base, channel)` re-derives which of the two forms an asset must match, from
  the **expected channel** — not from the config.

The anchor is now independent of which config was passed, which is what CPE-1908's design requires.
Measured: sabotaging this back to the naive `product_token(conf_product_name)` anchor reddens **10
tests, 4 of them legitimate-manifest controls**. Two new fixtures pin the exact CPE-1908 invocation
shape (plain `--conf` + `--expect-channel sidecar`) in both directions.

**Two of CPE-1908's own fixtures needed correcting**, both because they were unrealistic rather than
because the new checks are wrong:

1. `a_uniform_sidecar_manifest_passes_with_expect_channel_sidecar` served a **`.dmg`** for
   `darwin-aarch64`. Tauri's updater cannot apply a `.dmg`; the real published manifest serves
   `.app.tar.gz` for every darwin key (the `.dmg` ships as a release asset but no platform entry
   ever points at it). Changed to the real, versionless `.app.tar.gz` shape.
2. `a_plain_asset_in_a_manifest_expected_sidecar_is_rejected_by_name` asserted on the old refusal
   wording and on the old `platform -> channel` offender separator. Updated to the new
   `PROPERTY FAILED -- release channel` wording and `platform: reason` separator. Its
   `!stderr.contains("windows-x86_64 ->")` assertion had become **vacuously true** under the new
   format, so it was tightened to `windows-x86_64:` rather than left passing for the wrong reason.

**Finding 4, sidecar half — and why "copy it verbatim" was the wrong instruction to follow
literally.** `release-sidecar.yml` is `workflow_dispatch`-only, so `github.ref_type` is `branch` on
every run. Copying `release.yml`'s `ref_type == 'tag'` predicate across would have produced a branch
that **never fires** — a guard that looks stronger than it is, i.e. precisely the decay CPE-1933 is
open about, planted deliberately. Instead both workflows now compute one named predicate,
`RELEASE_BUILD` ("is this run cutting a release?"), from their own trigger:

- `release.yml`: `${{ github.ref_type == 'tag' }}` — tag-push triggered, so a tag ref is a release.
- `release-sidecar.yml`: `"true"` — dispatch-only with a **required** release-tag input, so every
  run cuts a release and there is no "nothing to verify" state at all.

The `run:` scripts are then **byte-identical**, and the vitest asserts that equality, converting
"the sidecar half mirrors release.yml" from a comment into a check. Neither workflow's downstream
`if:` changed, so CPE-1908's `SIGNING_KEY_STEP_IF` needed no edit and the coverage ratchet cannot go
red on its own remedy.

The guard was extended to cover **both** workflows explicitly rather than testing one and assuming
symmetry, per the same CPE-1933 reasoning. It extracts each step's `run` from its own YAML,
executes it under bash, and asserts on the exit code and `$GITHUB_OUTPUT` contents. The asymmetry
that genuinely exists (only `release.yml` has a reachable non-release arm) is asserted explicitly
rather than papered over.

**Sidecar red-proof.** Restoring the permissive one-line sig-detect step in `release-sidecar.yml`
reddens **2 tests**: `release-sidecar.yml: FAILS (non-zero exit) when cutting a release with no
signing key` on the **exit code** (`expected +0 not to be +0`), and the byte-identical-script drift
check. Reverted after measuring.

**Merged-state verification** (rebased branch, not the two branches separately):
`npm test` → **339 files / 4,700 tests, 0 failures**; `npm run check` → 0 errors, 0 warnings;
`cargo clippy --locked --all-targets -- -D warnings` clean; `cargo test --locked` → 112 pass, 0 fail;
lockfile pre-flight → `0 stale of 17 checked`.

The macOS residual (a `.app.tar.gz` bound to the release only by url prefix and signature) remains
deferred and is **not** filed here — the Foreman is filing it as its own ticket.

### 2026-08-27 (round 3) — SEC-1: the version binding read the wrong name

Security Auditor and Reviewer independently found the same blocker: the version binding rested on
the **uploaded asset filename**, which is attacker-chosen in this guard's own declared threat model
(release-asset write, no signing key). The old 0.1.0 installer, byte-identical and with its genuine
signature, only had to be uploaded under a current-looking name to pass at exit 0. The claim in the
PR body, the module doc and RELEASING.md — that this closed the downgrade for an asset-write
attacker — was therefore false for every platform, not just macOS.

**Ground truth first, again, and it changed two decisions.** Before relying on the remedy both
reviewers proposed, I downloaded the real `.sig` assets from `v0.57.69-sidecar` and read their
trusted comments:

```text
file:Cross-Platform Explorer (Sidecar)_0.57.69_x64-setup.exe
file:Cross-Platform Explorer (Sidecar)-0.57.69-1.x86_64.rpm
file:Cross-Platform Explorer (Sidecar).app.tar.gz      <- NO VERSION
file:Cross-Platform Explorer.app.tar.gz                <- NO VERSION
```

Two consequences neither reviewer had:

1. **The macOS exemption cannot be deleted.** Both suggested the trusted comment would remove it.
   It does not: macOS's *signed* name is versionless too, so there is no version anywhere to bind
   that artifact kind against. **CPE-1942 must stay open.** What the trusted comment does is shrink
   the exemption from "any signed bytes at all, uploaded under a macOS-looking name" — the
   auditor's demonstrated attack — to "a genuinely-signed macOS app tarball from another release of
   this same product", because the exemption is now keyed on the **signed** name.
2. **The `file:` value carries the unsanitised product name** (`Explorer (Sidecar)_`), which the
   uploaded asset name never has. That is a second, signature-covered channel signal, so the
   channel check now runs a second time over the signed names — catching a plain artifact uploaded
   under a perfectly sidecar-looking, correctly-versioned name.

**Where the decision now lives, and why there.** Inside `verify_update_manifest`, immediately after
each artifact's `minisign::verify` returns `Ok`. Ordering is load-bearing: a trusted comment is only
trustworthy once the global signature over it has been checked (`minisign::verify` does
`ed25519::verify(sig_and_trusted_comment, pk, global_sig)` — confirmed in the crate source, not
assumed). Putting it inside the function whose `Ok` every caller already gates on also means it
cannot be dropped at a call site the way a separate follow-up check could.

The name-based version binding was **deleted rather than kept alongside**: the asset name adds
nothing an attacker cannot forge, and two copies of an anti-rollback rule reading different inputs
could only ever disagree. `Ok(())` became `Ok(VerifiedManifest)` so the binary can report exemptions
and re-check the channel from the authenticated names.

A signature with **no** `file:` field is refused, not admitted — minisign's own `verify` accepts a
signature carrying no trusted comment at all (`(None, None) => {}`), so treating an absent one as
acceptable would hand the bypass straight back.

**Also fixed this round (all Reviewer findings, all in files already being touched):**

- `version` was read with `Some(v) => v.to_string()`, accepting `"version": ""`, while `pubkey` and
  `productName` two lines away both checked. `artifact_binding`'s doc asserted "the binary refuses
  before reaching here" — it did not. Both the check and the doc are now honest.
- `h2_an_unrecognised_platform_key_is_refused` was itself **shadowed**: it asserted only the
  platform name, which the version binding's own `UnknownPlatformKey` also produces, so it stayed
  green with the mapping check disabled. It now names the property and isolates.
- RELEASING.md claimed "a refusal always names which property failed", true only of the three new
  checks. Narrowed, and the residual re-scoped to what it actually is.
- Clippy caught literal tabs in the doc examples (the real trusted-comment strings). Escaped rather
  than suppressed.

**Red-proofs, all on the merged tree, each reverted after measuring:**

| Sabotage | Red |
|---|---|
| Bind against the uploaded basename again (the pre-SEC-1 rule) | **6** — 5 on the **exit code**, incl. both rename spellings and the macOS-exemption abuse |
| `if false &&` the platform/asset mapping refusal | **3** — including the newly de-shadowed unknown-key test |
| Revert `base_product_token` to the naive anchor | **10** — 4 of them legitimate-manifest controls |
| Restore the permissive sidecar sig-detect one-liner | **2** — one on the **exit code** |
| Accept an empty `version` again | **1** (see below) |

The empty-version sabotage initially reddened **nothing** — a guard with no red-proof, exactly the
shape this ticket exists to remove. I added
`an_empty_version_in_the_config_is_refused_even_for_a_darwin_only_manifest` (the darwin-only shape
the Reviewer identified as the exploitable one) and re-ran the sabotage; it now reddens.

**The Reviewer's 5-offender measurement is landed as a fixture.** Against the **real published**
`v0.57.69-sidecar` manifest, checked as `--conf` plain + `--expect-channel sidecar`, the naive anchor
produced 0 offenders and the correct one produces exactly the 5 real plain-channel entries
(`darwin-x86_64`, `darwin-x86_64-app`, `windows-x86_64`, `windows-x86_64-msi`,
`windows-x86_64-nsis`), each named as `WrongChannel(Plain)`. A control checks the same manifest as
plain and gets the other 6.

**Merged-tree verification:** `cargo clippy --locked --all-targets -- -D warnings` clean;
`cargo test --locked` 129 pass / 0 fail; `npm run check` 0/0; `npm test` 339 files / 4,700 tests /
0 failures; lockfile pre-flight `0 stale of 17`.
