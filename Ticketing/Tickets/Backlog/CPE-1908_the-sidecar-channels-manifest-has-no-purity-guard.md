---
id: CPE-1908
title: the channel-purity guard runs only on the plain manifest — the sidecar channel, which users actually install, has none
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-26
---

## Summary

CPE-1894 fixed the trigger that let plain and sidecar installers land in one release, and added
`platforms_with_mismatched_channel` — a real guard that reads the produced manifest's own asset URLs
and names every platform served from the wrong channel. Verified working against the live contaminated
manifest by both its reviewer and its UAT.

**It only ever runs against the plain channel.**

`verify-release-artifacts.rs` derives the expected channel from whatever `--conf` points at, and
`release.yml` always passes `src-tauri/tauri.conf.json` — the plain config. `release-sidecar.yml` never
invokes that binary at all; it runs a different check entirely. So the channel that **actually reaches
users** has zero channel-purity assertion on its manifest, and only the plain channel — whose accidental
contamination is the *less* consequential direction — is guarded.

Both independent legs flagged this separately, without seeing each other's report. Neither blocked
CPE-1894 on it, correctly: that ticket promised the trigger fix plus a checkable guard, and delivered
both. This is the follow-through.

## Why it matters, traced concretely

CPE-1894's UAT established the mechanism rather than assuming it:

- `plugins.updater.endpoints` is a single URL in the base config, and `tauri.sidecar.conf.json`
  overrides only `productName`, `identifier` and `bundle.createUpdaterArtifacts` — **not** the endpoint
  and **not** the pubkey. So a sidecar-build app checks the same endpoint as a plain one.
- Both workflows sign with the same `TAURI_SIGNING_PRIVATE_KEY`, and the pubkey baked into both builds
  is identical.
- Therefore a sidecar app on an affected platform fetches the manifest, resolves its platform key to a
  **plain** asset, and the updater's signature check **passes** — it is a genuine signature from the
  right key, for the wrong product. Nothing in that check distinguishes channel, and the version string
  is identical either way, so there is no visible signal to the user.

What could not be established without running an installer, and is worth resolving: whether the plain
installer overwrites the sidecar install in place (losing the AI Console) or lands as a second app,
since the two channels carry different `identifier`s. Both outcomes are bad; they differ in how bad.

## Acceptance criteria

- [ ] Run the channel-purity check against the **sidecar** manifest too, with the sidecar config as its
      expected-channel source. Wire it into `release-sidecar.yml` the way CPE-1894 wired the plain one.
- [ ] Make it **preventive** rather than detective if you can. On the plain channel the check is
      `needs: release`, so installers are already uploaded to the draft by the time it runs; on the
      sidecar channel there is a `needs:`-gated pattern available (CPE-1873's `verify-updater-pin`
      demonstrates it) that skips build/sign/publish outright on failure. Prefer that shape.
- [ ] Red-proof it: construct a mixed sidecar manifest, confirm the guard names the mismatched platforms
      and fails the job, and confirm a uniform sidecar manifest passes.
- [ ] Decide what happens to a manual `gh release edit --draft=false`. The plain channel's only
      protection today is `run.md`'s check of the job's conclusion — a hand-run publish bypasses it
      entirely. Say whether that is acceptable and why, or close it.

## Notes

Filed 2026-08-26 from CPE-1894's independent reviewer **and** its UAT, which raised it separately.

Related: **CPE-1894** (the trigger fix and the guard), **CPE-1909** (the case-sensitive negation gap
found in the same review), **CPE-1872** / **CPE-1874** (the plain channel's verification history),
**CPE-1873** (the `needs:`-gated preventive pattern worth copying).

See [[always-install-sidecar-build]] for why the sidecar channel is the one that matters here: the
standing rule is that installs and updates always use it, so an unguarded sidecar manifest is the guard
gap that can actually reach a user.
