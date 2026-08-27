---
id: CPE-1894
title: release.yml fires on `-sidecar` tags too, so one live manifest mixes plain and sidecar installers
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-26
---

## Summary

`release.yml`'s tag pattern is `v*`. That matches `v0.57.69-sidecar` just as happily as `v0.57.69`,
so the **plain** build's workflow also fires on **sidecar** tags and merges its plain installers into
the sidecar draft release.

This is not theoretical — it is visible in the live manifest right now. `latest.json` for the
published release has `linux-x86_64` and `darwin-aarch64` pointing at `…Sidecar…` assets, while
`windows-x86_64` and `darwin-x86_64` point at plain `Cross-Platform.Explorer_…` assets. **One
manifest, two different applications.**

The user's standing rule is that installs and runs must always use the sidecar (AI Console) build.
A manifest that hands some platforms the plain build breaks that silently, through the auto-updater,
on machines nobody is watching.

Found 2026-08-26 by CPE-1873's independent Security Auditor, while mapping which workflows run on
which events.

## Acceptance criteria

- [ ] Make the two channels' tag patterns disjoint, so a `-sidecar` tag runs only the sidecar
      workflow and a plain tag only the plain one. Decide the pattern deliberately and record it —
      `v*` excluding a suffix is easy to get subtly wrong.
- [ ] Repair the currently-published manifest, or state explicitly why an already-shipped mixed
      manifest is left alone. Users on the affected platforms may already have taken a plain-build
      update through it — say what happens to them.
- [ ] Add a guard that fails when a produced manifest's assets are not all from one channel. Assert
      on the **generated manifest**, not on the workflow's tag pattern — the pattern is what was
      wrong, so a test that reads it would have agreed with the bug.
- [ ] Red-proof it: construct a mixed manifest, observe the guard go red naming the mismatched
      platforms, restore.

## Notes

Interacts with **CPE-1893** (the `catalog` job skipped behind the same failing `release` job) and
**CPE-1874** / **CPE-1872** (signature verification on the release path). All four were surfaced by
the same audit and all four live in the release pipeline — worth sequencing together, but they are
genuinely separate defects and should not be collapsed into one ticket.

See [[always-install-sidecar-build]] for why a plain-build asset reaching a user is the specific harm
here, rather than a cosmetic packaging inconsistency.

## Work Log

- **2026-08-26 USMST** — Picked up by a sprint Worker. Plan:
  1. `release.yml`'s `on.push.tags` becomes `["v*", "!v*-sidecar"]` — GitHub Actions' documented
     include+negate-in-one-list filter form (not the separate `tags`/`tags-ignore` keys, which
     cannot be combined for the same event). `release-sidecar.yml` stays `workflow_dispatch`-only;
     it never listens on `push` at all, so nothing symmetric is needed there — the only overreach was
     the plain workflow's `v*` catching the sidecar tag too.
  2. Add a channel-purity check to `crates/updater-verify` — a pure function over the parsed
     manifest (asset URL basename contains `sidecar`, case-insensitive, per the real overlay-built
     filenames `release-sidecar.yml` produces vs. the plain `Cross-Platform Explorer_...` names) —
     wired into the existing `verify-release-artifacts` binary so the already-running
     `verify-published-manifest` job in `release.yml` fails loud, naming the offending platforms, on
     a mixed manifest. Unit tests red-prove it directly (construct the exact mixed shape from the
     live bug, assert the named offenders, assert a uniform manifest passes).
  3. Investigate whether the already-published mixed manifest can be repaired via `gh` without a
     new build, or must be documented as a known-bad manifest superseded by the next tagged release.
- **2026-08-26 USMST** — Implemented:
  1. `release.yml`'s `on.push.tags` is now `["v*", "!v*-sidecar"]` — the GitHub-documented
     include+negate-within-one-list form. `release-sidecar.yml` unchanged (it was never listening on
     `push` — the header comment there confirms it's `workflow_dispatch`-only — so the overreach was
     always one-directional and only `release.yml` needed the fix). Verified the tags filter parses
     as a valid two-entry list with `python3 -c "import yaml; ..."` against the live file.
  2. Added `crates/updater-verify/src/lib.rs::{Channel, channel_of_asset_url,
     expected_channel_from_product_name, platforms_with_mismatched_channel}` — infers a manifest
     platform's channel from its asset url's basename (`sidecar`, case-insensitive — confirmed
     against the real filenames both channels actually produce: plain `Cross-Platform.Explorer_...`
     vs `Cross-Platform.Explorer.Sidecar._...`), and flags every platform whose channel disagrees
     with the one implied by the conf's own `productName`. Wired into
     `verify-release-artifacts` (the binary `release.yml`'s existing `verify-published-manifest`
     job already runs post-matrix against the real published manifest) as an unconditional check —
     no new flag, no new job, no new dependency. 7 new unit tests in lib.rs (including
     `the_live_mixed_manifest_shape_is_caught_and_named`, which reconstructs the EXACT
     `windows-x86_64`/`darwin-x86_64`=plain, `linux-x86_64`/`darwin-aarch64`=sidecar shape the real
     v0.57.69-sidecar manifest shipped with, and asserts the guard names precisely those two
     offending platforms) + 3 new end-to-end tests in `tests/release_guard.rs` driving the real
     binary (RED: a sidecar asset under a plain `productName` fails with `windows-x86_64` named in
     stderr; GREEN x2: a channel-consistent plain manifest and a channel-consistent sidecar manifest
     both still pass). Full crate suite: 48/48 green (27 lib unit + 3 pin-guard + 18 release_guard).
     `cargo clippy --all-targets -- -D warnings` clean (this crate has no feature flags to run in
     "both modes" — it's a standalone, dependency-free-of-the-app crate per its own Cargo.toml doc
     comment).
  3. **Disposition on the already-published manifests — investigated with `gh`, left alone,
     documented here in full because the picture turned out bigger than the ticket's single
     example:**
     - Pulled `latest.json` for the current `/releases/latest/` sidecar release (`v0.57.69-sidecar`)
       plus two prior ones (`v0.57.68-sidecar`, `v0.57.67-sidecar`) directly via `gh release
       download`. **All three are mixed**, not just the one the ticket cited. The specific platforms
       affected are NOT fixed release-to-release — they depend on which of the two concurrently-
       running workflows' matrix legs happened to finish (and overwrite tauri-action's merged
       `latest.json`) last for a given platform key: v0.57.69 had `linux-x86_64` + `darwin-aarch64`
       correct (sidecar) and `windows-x86_64` + `darwin-x86_64` wrong (plain); v0.57.68 and
       v0.57.67 both had ONLY `darwin-aarch64` correct — `windows-x86_64` AND `linux-x86_64` were
       ALSO plain on those two. `darwin-aarch64` being consistently correct (sidecar) across all
       three, while `darwin-x86_64` is consistently wrong (plain) across all three, isn't
       coincidence: the plain macOS leg builds one **universal** binary and tauri-action writes that
       single artifact under BOTH `darwin-x86_64` and `darwin-aarch64` keys, while the sidecar
       channel's macOS leg is native-arch-only (see `release-sidecar.yml`'s own header comment: "not
       a universal binary, a preview-channel limitation") and so only ever has a `darwin-aarch64` key
       to overwrite with — it structurally can never win `darwin-x86_64` back, fix or no fix.
     - **Consequence for affected users, stated plainly:** any machine on the sidecar (AI Console)
       channel whose platform key currently resolves to a plain asset will, on its next auto-update
       check against `v0.57.69-sidecar`, silently install the plain, sidecar-free build under the
       version number `0.57.69` — with nothing in the UI to flag that anything changed, since the
       version string matches what they expected. They lose the AI Console until they notice and
       reinstall the sidecar build by hand (see `[[always-install-sidecar-build]]`).
     - **Decision: left alone, not hand-patched, for two concrete reasons found during
       investigation** (not just caution in the abstract):
       1. It genuinely **can't be fully repaired in place**. I confirmed a real,
          correctly-signed sidecar Windows installer + `.sig`
          (`Cross-Platform.Explorer.Sidecar._0.57.69_x64-setup.exe[.sig]`) IS already sitting on the
          `v0.57.69-sidecar` release, uploaded but simply not referenced by `latest.json` — that key
          alone could be spliced back in. But `darwin-x86_64` has **no sidecar-built counterpart to
          splice in at all**, for this tag or any recent one (see the universal-vs-native-arch
          mechanism above) — a "repair" would have to either drop that platform from the manifest
          entirely (removing update coverage a real client may currently depend on, on a release
          nobody asked me to touch) or leave it wrong. A partial fix that's silently still wrong for
          one platform is worse than a fully-documented gap.
       2. Because **every recent sidecar release is equally contaminated**, there is no clean older
          release to fall back to either — demoting `v0.57.69-sidecar` off `/releases/latest/`
          (e.g. `gh release edit --prerelease`) would only hand every platform the *next*-most-recent
          equally-mixed manifest instead, at the cost of freezing the update channel for the
          platforms `v0.57.69-sidecar` currently serves correctly (`linux-x86_64`,
          `darwin-aarch64`). That trade is strictly worse, not better, so it was not done.
     - **What actually fixes this going forward:** the tag-trigger fix in this PR makes the very next
       `vX.Y.Z-sidecar` dispatch produce a manifest `release.yml` never touches at all — a genuinely
       clean, sidecar-only manifest (still missing a `darwin-x86_64` key by design, per the
       native-arch limitation, which is expected and unrelated to this ticket). That release
       supersedes `v0.57.69-sidecar` as `/releases/latest/` the moment it's published, which — given
       this project cuts sidecar releases roughly daily — should self-heal the channel quickly.
       Flagging for the Foreman/release owner: cut and publish a fresh `-sidecar` release soon after
       this PR merges; until then, Windows/Intel-Mac sidecar installs that auto-update are at risk of
       the silent plain-build downgrade described above.
  Rebased onto `main`, ran the full `cpe-updater-verify` suite + clippy clean, opening the PR.
