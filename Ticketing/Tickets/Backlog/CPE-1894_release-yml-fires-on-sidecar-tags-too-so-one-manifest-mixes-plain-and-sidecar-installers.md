---
id: CPE-1894
title: release.yml fires on `-sidecar` tags too, so one live manifest mixes plain and sidecar installers
type: bug
priority: High
status: Backlog
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
