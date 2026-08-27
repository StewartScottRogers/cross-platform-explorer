---
id: CPE-1894
title: release.yml fires on `-sidecar` tags too, so one live manifest mixes plain and sidecar installers
type: bug
priority: High
status: Doing
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
