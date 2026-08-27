---
id: CPE-1942
title: the macOS `<productName>.app.tar.gz` carries no version in its name, so one asset per release escapes the version binding CPE-1923 built
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

CPE-1923 (PR #1053) bound every release asset to its release by requiring the version to appear in
the asset's name — closing a **signed downgrade** that previously passed the verifier at exit 0.

**One asset per release cannot satisfy that rule.** The macOS updater artifact is named
`<productName>.app.tar.gz` with **no version in it at all**. So it is bound to its release only by
url prefix and signature — both of which a genuine older release also satisfies. It is exactly the
hole CPE-1923 closed everywhere else.

The author granted it an explicit exemption rather than either failing every macOS release or
pretending the binding was complete. That was the right call, and the exemption is **printed every
time it is granted** so it cannot be silently relied on. This ticket is to close it properly.

## Why it was deferred rather than fixed

Closing it means reading `CFBundleShortVersionString` out of the tarball — which means tar and gzip
dependencies inside `crates/updater-verify`, a crate deliberately confined to a single crypto
dependency. That is a real cost against the lean-core guardrail and deserves its own decision, not a
line in someone else's PR.

## Acceptance criteria

- [ ] Decide, and record, how to bind the macOS asset. Options worth weighing, cheapest first:
      - have the **release workflow** rename or record the version alongside the asset, so the
        verifier never needs to open the tarball at all — this is probably the answer, and it costs
        the crate nothing;
      - read `CFBundleShortVersionString` from the tarball, accepting the tar+gzip dependencies;
      - bind by **content hash** recorded at build time, which sidesteps naming entirely.
      If a dependency is added, the Dependency Steward's question applies: is it justified, or can
      the lean core absorb this another way?
- [ ] **Whatever is chosen, remove the exemption** rather than leaving it printed-but-permanent. A
      loud exemption is a good interim state and a bad final one.
- [ ] **Demonstrate the downgrade first**, on the macOS asset specifically: a genuine older
      `.app.tar.gz` with its real signature, published under a newer release, must be shown to pass
      today. Then show it refused after the fix, and show a legitimate macOS release still accepted.
      Both directions.
- [ ] Check whether **any other asset** in any channel lacks a version in its name — enumerate the
      real published assets rather than reasoning from the bundler config (CPE-1923's author found
      the ticket's own assumptions about asset names were wrong twice, in ways that would have
      rejected 100% of real sidecar assets). `gh release view` is the source of truth.
- [ ] Keep the exemption **printed** until it is genuinely gone, and make sure a test asserts it is
      printed — a silent exemption is worse than none.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1053's Worker, which recorded the residual rather
than hiding it and asked for a follow-up. That PR's other deferral — the `release-sidecar.yml` half —
was **not** deferred once PR #1039 merged and made those lines exist; it was folded back into #1053.

Related: **CPE-1923** (the version binding, PR #1053), **CPE-1908** (the channel-purity guard),
**CPE-1941** (the other route a correctly-signed bundle can carry stale content), **CPE-1872**
(release-workflow wiring).
