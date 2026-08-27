---
id: CPE-1943
title: three loose matches in the release verifier — the version token accepts prerelease siblings, the channel check is a bare prefix test, and platform keys parse case-insensitively
type: bug
priority: Low
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Three non-blocking findings from PR #1053's independent Security Auditor, raised alongside the two
blocking ones (the rename bypass and the merged-state anchor regression) that PR fixed. **All three
are latent** — none is exploitable against what ships today — and all three are the same shape: a
match that is looser than the code reads.

Each was found by feeding the verifier adversarial input, not by reading it.

## F-A: the version token accepts prerelease and build siblings

`basement_carries_version`'s boundary set treats `-` and `+` as delimiters. **It has to** — the RPM
asset is spelled `…-0.57.69-1.x86_64.rpm`, so `-` cannot be part of the token. The consequence is
that with `0.57.70` shipping, all of these exit **0**:

    …_0.57.70-rc1_…      …_0.57.70-beta.2_…      …_0.57.70+evil_…

So a prerelease build can be substituted for the final release. This is not hypothetical housekeeping:
`release.yml`'s own trigger comment explicitly contemplates a future `-rc1`/`-beta` tag convention,
which would make the sibling a real artifact rather than a fabricated one.

## F-B: the channel check is a prefix test with one hard-coded exclusion

The anchored channel check asks whether the asset token *starts with* the expected product token, and
excludes exactly one thing: the literal `format!("{expected_token}sidecar")`.

    Cross-Platform.Explorer.Enterprise._0.57.70_x64-setup.exe   -> exit 0 against a PLAIN manifest

Any future third product or channel whose name begins with the plain one is silently accepted. Note
this is the *same root cause* as the blocking SEC-2 that PR #1053 fixed — the plain token being a
strict prefix of the sidecar token — surviving in the general case after the specific one was closed.

## F-C: platform keys parse loosely

`WINDOWS-x86_64`, `Windows-x86_64-nsis` and a bare `windows` all resolve and pass at exit 0.
(`-windows-x86_64` is correctly refused.) Not exploitable alone — the version binding and mapping
checks still apply — but it widens what an attacker can put in the key field while staying
recognised.

## Acceptance criteria

- [ ] **F-A:** decide what a version token should accept, and record it. Options: require the token
      to be followed by a delimiter that is *not* `-`/`+` **except** in the RPM's known shape; or
      recognise the RPM spelling explicitly rather than loosening the general rule for it. Prefer the
      second — a special case named once beats a general rule weakened everywhere. Whatever is
      chosen, **`-rc1`, `-beta.2` and `+evil` must be refused while the real RPM still passes.**
- [ ] **F-B:** make the channel test an exact identity rather than a prefix with an exclusion list.
      A prefix test needs a new exclusion for every product ever added, which is a rule nobody will
      remember to update — CPE-1932's lesson in a different costume.
- [ ] **F-C:** decide whether loose platform-key parsing is wanted. If it is, say so at the site; if
      not, tighten it. Do not leave it undocumented either way.
- [ ] **Red-proof each** by feeding the verifier the exact inputs above and asserting on the **exit
      code** first and unconditionally, then confirm the real published assets still pass — the
      9-platform sidecar and 6-platform plain shapes both. A verifier that refuses a real release is
      a release-pipeline outage, and this repo has already lost 27 days to one.
- [ ] Read the **real** asset names off published releases before changing any rule. PR #1053 found
      the ticket's own assumptions about asset names were wrong **twice**, in ways that would have
      rejected 100% of real sidecar assets. `gh release view` is the source of truth, not the
      bundler config and not a ticket.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1053's Security Auditor (findings SEC-4, SEC-5,
SEC-8). Its two blocking findings were fixed inside that PR.

Related: **CPE-1923** (the verifier work, PR #1053), **CPE-1908** (the channel-purity guard),
**CPE-1894** (a sidecar asset in a plain manifest), **CPE-1942** (the macOS asset with no version in
its name), **CPE-1941** (stale content under a newer version).

## F-D added 2026-08-27 — the mapping rule is OS-granular, so same-OS payload substitution is admitted

From PR #1053's round-4 audit, after that PR closed the cross-OS case (SEC-9). The new signed-name
mapping pass asks whether the payload's extension suits the key's **operating system**. It does not
ask whether it suits the key's **bundle kind**, although the key carries it:

    windows-x86_64-nsis  served the genuine .msi        -> exit 0
    linux-x86_64-deb     served the genuine .rpm        -> exit 0
    linux-x86_64-rpm     served the genuine .AppImage   -> exit 0

Same denial-of-update class as SEC-9, one notch narrower: an NSIS client is handed an MSI it cannot
apply. The `-nsis` / `-msi` / `-deb` / `-rpm` suffix is available and unused.

**This is scope, not slippage** — CPE-1923's own proposed fix was OS-granular, so PR #1053 delivered
what was asked. Recorded here so the narrower case is not mistaken for closed.

Add to the acceptance criteria: **decide whether the mapping should be bundle-kind-granular**, and if
so use the suffix already present on the key. Red-proof with the three rows above, and keep the
18-case matrix that PR #1053's audit established (bare `.tar.gz`, bare `.gz`, `.app.tar`, extension
mid-name, trailing dot, and the case variants `.EXE` / `.App.Tar.GZ` / `.AppImage` which must all be
**accepted**).
