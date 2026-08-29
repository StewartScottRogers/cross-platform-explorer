---
id: CPE-1951
title: a release cut from an **older** commit publishes a fully green catalog that every client silently refuses — the floor is a static ratchet, not a monotonic one
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

CPE-1941 (PR #1061) fixed the old-tag republish by deriving each catalog entry's `version` from the
**tagged commit's committer timestamp** instead of a publish-time clock. That is right, and it
introduces a consequence the PR does not close: **the version now tracks commit order, not release
order.**

Cut a release from a commit older than the last released one — a hotfix on a maintenance branch, a
revert branch, `git tag` on a non-tip commit, or re-cutting an earlier tag to fix an asset — and you
stamp a *lower* version.

Concretely: `v0.57.70` from `%ct = 1787200000`, then `v0.57.71` off an older base at `1787100000`.

**What gets published:** a fully valid bundle. The floor check passes (well above `1787000000`), the
future check passes, index and per-manifest signatures verify, sha256 binds, `gh release upload`
succeeds. **The job is green.**

**What clients do:** every entry returns `ApplyOutcome::Rollback`. Nothing is written. Nothing is
logged as a release failure.

**Found independently by both of PR #1061's gates** — the Security Auditor and the Reviewer, neither
seeing the other's report. Under the old `date +%s` scheme this failure mode did not exist.

## Why it is silent

Nothing on the publish side compares the derived version against the **last published** version. The
floor (`CATALOG_VERSION_FLOOR`) is a **static** ratchet — it only rejects versions below a fixed
constant — not a monotonic one.

Detection today is `catalog-freshness.yml`'s 14-day `now - version` alarm, which only fires if the
older base commit is already past the threshold. **A hotfix off a three-day-old base stays green
while every client refuses it**, indefinitely.

It is an **availability** failure, not a security one — nothing unsafe is accepted — but it is the
kind that surfaces as "why has nobody's agent catalog updated in a month".

## Mitigating today, and why that is not enough

`scripts/release.ps1:515` does `git push origin HEAD --tags`, so tags are cut from the current HEAD
and the linear process cannot hit this by accident. It bites a **deliberate** off-tip tag — which is
exactly what someone reaches for under pressure, during a hotfix, when they are least likely to
notice a silent client-side refusal.

## The fix shape, from CPE-1941's author (who was asked to pick, and argued for it)

**Take the index-fetch lower-bound check, not a `LAST_PUBLISHED_VERSION` counter:**

1. A committed counter must be bumped by something. Auto-bumping means the release job commits back
   to the repo from a detached tag checkout; manual means it rots — and **a stale counter degrades
   into exactly the static ratchet we already have.**
2. **The fetch is not the trust dependency CPE-1924 rejected.** That objection was about *trusting*
   fetched content to decide what to publish. This uses it only as a **lower bound that fails the
   build**, and the job already runs `gh release upload` against the same host, so it is not even a
   new egress class.

   > **Corrected in #1091 round 2 — this bullet used to end "a hostile or garbage response can cause
   > a false *failure*, never a false success. It fails closed, so it needs no signature
   > verification to be safe." That is measurably false**, and it is the sentence that licensed
   > shipping an unverified fetch on the release path. Two review gates independently produced
   > parseable responses reaching **exit 0**. Two were bugs and are fixed: a bound above 2^63-1 made
   > `[ -le ]` *error* rather than compare false (bash's `test` prints `integer expected` and
   > returns 2, the refusal branch is skipped, and the success `printf` runs), and jq's `max` sorts
   > numbers below strings, so **one** string-typed `version` masked every numeric one in the whole
   > index. Two remain **by design**: the positively-enumerated empty-release branch, and an index
   > that simply reports a lower version than the truth — a bound you fetched is a bound the server
   > chose. The claim that holds is the narrow one: *every route where the fetch did not produce a
   > usable answer is fatal.* Defeating the guard reverts to pre-CPE-1951 behaviour; it does not
   > forge a catalog, because the signing key is not in this step's env.
3. It closes the legacy window in the forward direction too: if an old-tag re-run ever stamped a
   large `date +%s`, the next real release would fail **loudly** instead of being silently refused
   everywhere.

**Implementation note from the same author:** resolve what clients resolve —
`releases/latest/download/catalog-index.json` — and handle the draft/`latest` distinction. During a
release run `latest` is still the *previous* release, which is exactly the bound wanted.

## Acceptance criteria

- [ ] Add a publish-time lower-bound check against the last published catalog index. It must be
      **fatal**, and it must fail closed on any fetch error — never skip the check because the fetch
      did not work. This repo has lost 27 days to a broken release workflow; a check that fails
      *silently* here is worse than the bug.
- [ ] **Demonstrate the bug first**: tag an older commit in a fixture, publish, and show a green job
      producing entries every client refuses. Assert on the client outcome and the on-disk state, not
      on a verdict enum.
- [ ] Then show it refused after the change, **and** a legitimate newer release still accepted. Both
      directions.
- [ ] Handle the `latest`/draft distinction explicitly — PR #1061's review found `v0.57.32` is a
      **draft**, never served to any client, which is exactly the sort of thing this check will trip
      over. Say what happens when `latest` has no catalog index at all — which is true **right now**:
      `/releases/latest/` resolves to `v0.57.69-sidecar`, and only the plain channel runs the
      `catalog` job at all, so the live index URL 404s (confirmed 2026-08-27 by
      `catalog-freshness.yml` run 33107498544 → issue #1062).
      **Record correction (CPE-1953):** the last release that actually PUBLISHED a catalog index is
      **v0.57.33**, not v0.57.32. An asset scan says v0.57.32 only because v0.57.33's release was
      later deleted; v0.57.32 is itself a draft no client ever fetched. The last successful
      `release.yml` run was v0.57.33 on **2026-07-25**.
- [ ] Keep `CATALOG_VERSION_FLOOR`. The static floor and the monotonic bound answer different
      questions and both are wanted.
- [ ] Red-proof the fetch failure path: a 404, a truncated body, a 500, and a timeout must each fail
      the build with a distinct message.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1061's Security Auditor (F1) and Reviewer (F2), which
found it independently of each other, plus the author's recommended shape.

Related: **CPE-1941** (the versioning change, PR #1061), **CPE-1940** (the fail-closed baseline, PR
#1058 — the index-fetch work would build on its `VerifiedIndex`), **CPE-1924** (where an index fetch
was first costed and rejected, for a different reason), **CPE-308** (the catalog pipeline),
**CPE-1953** (no release since **v0.57.33** publishes a catalog index at all — corrected there from
v0.57.32).

## Work Log

### 2026-08-28 — implemented as specified (index-fetch lower bound; the counter was not revisited)

**What shipped**

- **`.github/workflows/scripts/catalog-lower-bound.sh`** (new). Resolves what clients resolve —
  `releases/latest/download/catalog-index.json` (`catalog_url()`, `src-tauri/src/lib.rs`) — and
  refuses, fatally, any candidate that is not **strictly greater** than the max `entries[].version`
  published there. `-le` and not `-lt` on purpose: at equality a client answers `AlreadyCurrent` and
  writes nothing, so a `-lt` gate would let a release publish that reaches no user. That boundary is
  *measured through the real engine*, not asserted.
- **`release.yml`'s `catalog` job**: a new fatal step `Refuse a catalog version that is not newer
  than the published one (CPE-1951)` (id `lb`, `timeout-minutes: 5`), placed **after** the CPE-1941
  derive and **before** `catalog-sign`, so a refusal publishes nothing. `Catalog publish outcome`
  now accounts for `steps.lb.outcome` alongside the other four.
- **`CATALOG_VERSION_FLOOR` kept.** Both are wanted and neither implies the other: the floor bounds
  against what the *installed base* holds (which no fetch can observe — a client may sit on an old
  catalog for months); the new check bounds against what is *published right now*.

**The 404 decision, and why it is not a bare skip**

A 404 on the index URL is the state of the world today (CPE-1953 / #1062). The script does **not**
skip on it. It **enumerates first**: `gh api repos/<repo>/releases/latest`, requires that call to
succeed, requires a `tag_name`, and requires `assets` to be a readable array. Only an enumeration
that *succeeded* and contained no `catalog-index.json` yields the `none` verdict (a `::warning::`,
then proceed). Everything else is fatal, including a 404 on the index URL **after** the asset list
said the asset is there — that is a contradiction, not an absence. Written at the site, at length,
with the instruction not to add a bootstrap escape hatch: an escape hatch is a skip wearing a coat.

**Evidence**

- Bug demonstrated first, on the client and on disk:
  `sidecar/host/tests/catalog_offtip_release_lower_bound.rs` (5 tests) — a hotfix committed off an
  older base is `ApplyOutcome::Rollback`, the manifest bytes stay at v2, the recorded baseline does
  not move, and re-fetching replays the identical refusal. Its sibling test shows the publish side
  is entirely green for that release (clears the shipped `CATALOG_VERSION_FLOOR`, clears the
  future-date check, signs a complete bundle).
- Publish side, executed against a real three-commit git fixture and stubbed `gh`/`curl`:
  `src/lib/catalogPublishLowerBound.test.ts` (33 tests) — the real `catalog-version.sh` derives the
  off-tip number *green*, the real lower-bound script then refuses it, and the re-cut tag is
  accepted. Ten fetch-failure causes, ten distinct exit codes, ten distinct first lines (asserted as
  a set, so two wordings collapsing reds).
- Red-proofs, recorded at each site: editing `catalog_url()`'s template in `lib.rs` reds the URL
  derivation; replacing the workflow step's invocation reds 5 structural tests, and hiding the
  invocation in a **trailing comment** keeps them red (`logicalLines`, not a whole-line filter);
  flipping `-le` to `-lt` reds the equality case.

**A trap found while writing it, worth carrying forward.** On Windows, prepending a stub dir to
`PATH` via node's `env` does **not** win: MSYS2's bash puts `/mingw64/bin` in front at startup, and
`curl.exe` lives there. The "stubbed" suite silently fetched the real github.com and reported ten
genuine 404s as ten distinct guard verdicts. Prepending inside bash with the `Z:/…` spelling is
worse — a Windows path contains a colon, the PATH separator. The fix is `cygpath -u` plus an
in-shell prepend, and `beforeAll` now **refuses to run** unless *both* stubs resolve under the stub
dir. Checking only `gh` is what missed it: `gh` won, `curl` did not.

**Docs updated**: `docs/design/CPE-308-agent-catalog-updates.md` (the "known gap" bullet became the
fix, with the counter rejection and the 404 reasoning), `docs/security/threat-model.md` (the
availability gap recorded as closed), and `catalog-version.sh`'s own header (which had told the next
reader the gap was open and not to close it by widening the floor).
