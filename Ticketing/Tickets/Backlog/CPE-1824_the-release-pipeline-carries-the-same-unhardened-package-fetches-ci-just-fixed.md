---
id: CPE-1824
title: the release pipeline carries the same unhardened package fetches CPE-1787 just fixed in CI
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

CPE-1787 hardened the five `apt-get` sites in `.github/workflows/ci.yml` — retries, connect+data
timeouts, and a per-step `timeout-minutes` cap so a stalled mirror fails fast instead of riding to
the 360-minute default (which actually happened, for 1.5+ hours, on PR #935).

The identical unhardened `apt-get update && apt-get install -y ...` pattern is still live in the
**release** pipeline, which is arguably higher stakes — a hang there blocks shipping signed
installers, and the release build is the one nobody is watching:

- `.github/workflows/release.yml:49-54` and `:201`
- `.github/workflows/release-sidecar.yml:130-138`

The same hang class also reaches `ci.yml` through other tools, which CPE-1787 deliberately scoped
out:

- `ci.yml:496` — `brew install ffmpeg`
- `ci.yml:501` — `choco install ffmpeg`
- `ci.yml:499`, `:511`, `:529` — pdfium `curl` fetches that pass `--retry` but **no `--max-time`**,
  so a *stalled* transfer is unbounded even though a failed one retries
- `release-sidecar.yml:322` — **added 2026-08-20 by CPE-1764**, a sixth `curl` (fetching BtbN's
  `checksums.sha256`) with no `--max-time`, no `--connect-timeout`, and no `timeout-minutes` on its
  step. Confirmed by the CPE-1764 reviewer as a new site this ticket must pick up. Note it also has no
  `--fail`, so a 404 returns exit 0 with an error page in the variable — that half is CPE-1764's to fix,
  but check it landed before assuming this site only needs a timeout.

## Why it matters

A silent hang in the release workflow is worse than one in CI: CI hangs are noticed because someone
is waiting on a PR, whereas a release build hangs against nobody's attention and the first symptom is
a draft release with no installer assets — the exact state `/run` has to defend against by checking
for assets before publishing.

## Acceptance criteria

- [ ] Every `apt-get` site in `release.yml` and `release-sidecar.yml` carries the same option set
      CPE-1787 established, and every step carries a `timeout-minutes` cap sized to its real duration.
- [ ] `brew` and `choco` sites get an equivalent bound — a step-level `timeout-minutes` at minimum,
      plus any retry/timeout options those tools genuinely support (check, do not assume they mirror
      apt's).
- [ ] Every `curl` fetch that can stall gets `--max-time` (and `--connect-timeout`), not just
      `--retry`. State the values chosen and why.
- [ ] The `ciAptGetHardening` guard is extended to cover the release workflows, or a sibling guard is
      added — whichever keeps the assertions readable. It must parse the YAML structurally through
      `src/lib/preview/yaml.ts`, the way CPE-1787's guard does, never regex-over-raw-text.
- [ ] Red-proof every assertion by deleting the single line it protects, observing red, and reverting;
      record the line for each.
- [ ] Verify the `continue-on-error` interaction per site the way CPE-1787 did: a cap under
      `continue-on-error: true` still fails fast and then gets swallowed, so the job outcome is
      unchanged; a cap without it converts a silent hang into a hard failure. Say which each site is.

## Notes

Found by the CPE-1787 Reviewer, which credited that PR for declaring its `ci.yml`-only scope rather
than presenting a partial sweep as complete — this is the declared remainder, not a defect in it.

Separately, CPE-1787's own regression guard is being widened in that PR to catch bare `apt` as well
as `apt-get`; whatever spelling coverage lands there should be reused here rather than re-derived.
