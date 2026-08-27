---
id: CPE-1909
title: the release tag negation is case-sensitive, so a mistyped `-Sidecar` tag reopens the mixed-manifest bug
type: bug
priority: Low
status: Backlog
tags: ready
estimate: XS
created: 2026-08-26
---

## Summary

CPE-1894 made the two release channels' tag triggers disjoint with
`tags: ["v*", "!v*-sidecar"]`. Verified correct: the include-then-negate form is right, the ordering is
right, and every realistic tag shape was checked.

**GitHub's tag filters are case-sensitive**, and `release-sidecar.yml`'s `tag` `workflow_dispatch`
input is free text with no validation or normalisation. So a human who types `v0.11.0-Sidecar` — or any
other casing of the suffix — produces a tag the negation does **not** exclude. `release.yml` fires on
it, and the mixed-manifest defect reopens for that one release.

This is not a regression CPE-1894 introduced; case was never handled anywhere in this pipeline. And it
needs a manual typo in a field that has always been typo-able. Its reviewer explicitly declined to block
on it for those reasons. But the whole point of that ticket was that the two channels must be
*structurally* incapable of mixing, and a case-sensitive comparison against free-text human input is not
structural.

## Acceptance criteria

- [ ] Normalise or validate the `workflow_dispatch` tag input — lowercase it, or reject a tag that does
      not match the expected shape, with a message naming what was expected. Prefer rejecting: silently
      rewriting what someone typed has its own surprises.
- [ ] Red-proof it: attempt a dispatch with `v0.0.0-Sidecar` (and one or two other casings) and confirm
      it is refused or normalised, without pushing a real tag or cutting a release.
- [ ] Decide whether the tag filter itself should also be made case-insensitive, or whether normalising
      at the input is sufficient. Record the reasoning — a tag can also be created by hand outside the
      dispatch, so input validation alone may not be the whole answer.
- [ ] Confirm `platforms_with_mismatched_channel` still catches the resulting manifest if a mistyped tag
      does slip through, so this stays a defence-in-depth gap rather than a single point of failure.

## Notes

Filed 2026-08-26 by CPE-1894's independent reviewer, classified non-blocking.

Related: **CPE-1894** (the disjoint triggers), **CPE-1908** (the sidecar manifest has no purity guard at
all — the larger half of the same story).

Worth pairing with CPE-1908, since both are about the same question: what stops a wrong-channel manifest
reaching a user, and is it a mechanism or a convention.
