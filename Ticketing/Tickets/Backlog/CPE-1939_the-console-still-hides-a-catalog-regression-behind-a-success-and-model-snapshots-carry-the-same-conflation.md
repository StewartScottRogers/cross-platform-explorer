---
id: CPE-1939
title: a catalog regression is still hidden behind "Updated 1 agent.", and the model snapshot carries the same `==`/`<` conflation CPE-1924 just fixed for agents
type: bug
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Two residuals surfaced by PR #1051's independent Reviewer while verifying CPE-1924. Both are
**pre-existing and unchanged by that PR** — the Reviewer measured that explicitly and did not let them
block it — and both are one branch over from the defect CPE-1924 exists to fix.

## F-A: `applied > 0` wins over every rejection branch, so a regression can ship silently

`refreshCatalog()`'s branch order is:

    applied>0 → error → offline → integrityRejected → regressedRejected → alreadyCurrent → indexOk → fallback

So a publish carrying **one genuine upgrade plus one regressed entry** reports **"Updated 1 agent."**
and never mentions the regression. The user is told about the success and not about the catalog going
backwards — which is the exact shape CPE-1924 was filed to fix, displaced from the `==`/`<` split
into the success branch.

CPE-1924 got the mixed *rejection* case right: with `alreadyCurrent: 3, regressedRejected: 1` the
amber "gone backwards" message wins, verified in the launcher's jsdom harness. The mixed
*success* case is the one still wrong. `integrityRejected` is masked the same way, which is worse —
a rejected signature is not something to swallow behind a success count.

## F-B: the model snapshot still conflates `==` with `<`

`sidecar/ai-console/src/model_snapshot.rs:148` still carries

    current_version.is_none_or(|v| incoming.version > v)

— the exact pattern CPE-1924 removed from the agent catalog, for the **model** snapshot (CPE-451).

This is **not** a second comparison that could disagree with `version_standing`: different crate,
different artifact, and the Reviewer confirmed nothing reports a user-visible *reason* off it, so
there is no live bug today. It carries the same latent conflation, and its doc comment now points at
an `is_upgrade_over` that no longer owns a comparison — so the comment is stale in the
provenance-claim sense (CPE-1933).

## Acceptance criteria

- [ ] Decide and record what a **mixed** publish should say. A success count must not erase a
      regression or an integrity rejection. Weigh "one line that mentions both" against "the worse
      news wins" — CPE-1924 chose worse-news-wins for the rejection branches, and consistency with
      that is worth something.
- [ ] `integrityRejected` in particular must never be masked by a nonzero `applied`. A rejected
      signature is the one outcome that should always be visible.
- [ ] Pin the mixed cases in the launcher's jsdom harness: `applied>0 + regressedRejected>0` and
      `applied>0 + integrityRejected>0` must each assert the message a user actually needs. Red-proof
      them by collapsing the branch order and confirming they redden.
- [ ] For F-B: either give `model_snapshot` the same `version_standing` treatment, or **delete the
      stale provenance comment** and state plainly that the conflation is deliberate and unreported.
      Do not leave a comment pointing at a function that no longer does what it says.
- [ ] While there: `is_upgrade_over` / `is_upgrade()` now have **zero production callers** in
      `sidecar/host` — trust flows entirely through `refusal()`. They are kept alive by
      `refusal_and_is_upgrade_agree_on_what_is_applyable`, which is the invariant test protecting the
      whole design. **Do not let a dead-code sweep delete them**, and say so at the site so the next
      sweep leaves them alone.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1051's independent Reviewer, which measured both as
pre-existing and explicitly declined to block on them.

Related: **CPE-1924** (the agent-catalog split, PR #1051), **CPE-1911** (the honest-status work),
**CPE-451** (the model snapshot), **CPE-1933** (provenance claims in comments).
