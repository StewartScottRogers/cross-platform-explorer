---
id: CPE-1939
title: a catalog regression is still hidden behind "Updated 1 agent.", and the model snapshot carries the same `==`/`<` conflation CPE-1924 just fixed for agents
type: bug
priority: Medium
status: Done
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

## Closing record — merged as PR #1100 (`2c7f69ff`), 2026-08-28

### F-A — a rejection can no longer hide behind a success count

`refreshCatalog()`'s branch order led with `applied > 0`:

```
applied>0 → error → offline → integrityRejected → regressedRejected → alreadyCurrent → indexOk → fallback
```

So a publish carrying **one genuine upgrade plus one regressed entry** rendered a green **"Updated 1
agent."** and never mentioned the regression — and `integrityRejected` was masked identically, **which is
worse: a rejected signature is not something to swallow behind a success count.** CPE-1924 got the mixed
*rejection* case right; this was the same defect displaced into the **success** branch.

**The decision, recorded above `refreshCatalog` and not only in the PR body: the worse news wins the
branch.** That is what CPE-1924 already chose when it put `regressedRejected` ahead of `alreadyCurrent`,
and **consistency with a decision taken one branch over beat writing a marginally different sentence.**
But the success is not erased either — **each rejection branch now names how many agents did update, in
the same sentence.**

Two supporting changes that are easy to miss and both load-bearing:

- `load()` is **hoisted out of the success branch** — re-rendering is a fact about `applied`, not about
  which message won.
- The regression branch's **denominator gains `applied`**, or `1 applied + 3 current + 1 regressed` would
  claim *"1 of the 4"* and **lose an entry**. Verified by construction, not on paper: the live render is
  *"1 of the **5** published agent entries is older than the version you already have, so it wasn't
  installed. **1 other agent was updated normally**…"*, and 3 applied + 2 regressed pluralises correctly.

**Branch consumers enumerated, not recalled.** `refreshCatalog()` returns nothing and both call sites
discard it; sweeping the response fields across `.rs/.ts/.html/.js/.mjs/.svelte` found two producers
(`src-tauri/src/lib.rs::do_fetch_catalog`, `sidecar/ai-console/src/console.rs::handle_catalog_refresh`).
The reorder is confined to what the user is told.

### The sabotage that is shadowed by the PRODUCER, not by the chain

Three CPE-1929 runs against `src/lib/ai-console-launcher.test.ts` (87 tests, green as shipped), reproduced
independently by the Reviewer:

| | change | result |
|---|---|---|
| **A** | `applied > 0` restored to the head of the chain | **85 passed / 2 failed** |
| **B** | `true \|\|` on the integrity predicate | **81 passed / 6 failed** |
| **C** | `true \|\|` on `r.indexOk` in **both** rejection branches | **87 passed / 0 failed — green** |

A's two failures are exactly the new mixed cases, **each on its *first* assertion** — verbatim
`expected 'Updated 1 agent.' to match /gone backwards/i` and
`expected 'Updated 2 agents.' to match /corrupted or mis-signed/i` — i.e. **on the masking, not on a
count.**

**C is green because the producer makes the shape unconstructible, and that was verified structurally
rather than asserted.** `catalog.rs:686` sets `report.index_ok = true` **after** all three early
`return report`, each returning `ApplyReport::default()` with `rejected` empty — so
`index_ok == false ⟹ rejected.is_empty() ⟹ all three counts 0`. All four exits in
`do_fetch_catalog`/`fetch_catalog_response` hard-code the counts to 0 or omit them, and
`broker_client.rs`'s `unwrap_or(0)` normalises absent to 0.

**And the Reviewer went one step further than "not a live fail-open":** it fed the *impossible* shape
(`indexOk:false, integrityRejected:2`) through the real chain and found it lands on the **conservative
refusal** branch. **So the conjunct fails closed — keeping it is right, not merely harmless.** The site
says a green sabotage here is **expected** and explicitly *"do not read C as evidence that A and B are
green too."*

### F-B — the stale claim deleted, the conflation kept, with the reason

`model_snapshot.rs`'s `accept_snapshot` still carries `current_version.is_none_or(|v| incoming.version >
v)` — the `==`/`<` conflation CPE-1924 removed from the agent catalog. Its doc comment pointed at
`is_upgrade_over`, which **since CPE-1924 delegates rather than compares** (the sole `cmp` lives in
`version_standing`), so the comment was stale in the provenance sense.

**The conflation stays, because a split would produce a distinction nothing can consume:**
`Console::refresh_snapshot` collapses all four rejection paths into one bare `false` (verified: exactly
four bare `return false`, fetch error / malformed JSON / verify failure / `accept_snapshot`), and its only
production caller, `handle_models`, **discards even that.**

**What would make it a live bug is written at the site**, with the instruction to derive a
`VersionStanding`-shaped return from one `cmp` at that point rather than adding a second comparison.

**And after review, one more clause was added, which is the sharper point:** that whole safety argument is
**an assertion about another file's control flow**. It was checked by inspection by author and reviewer on
2026-08-28 and is **pinned by no test** — so the site now says the next reader should re-grep
`refresh_snapshot` rather than trust it, and says why it is not derived (parsing another module's control
flow for a one-caller helper). *The two "zero production callers" notes rot toward over-caution; this one
would rot toward keeping an unsafe simplification for a reason that stopped being true.*

### Dead code kept on purpose

DO-NOT-DELETE notes on `VersionStanding::is_upgrade`, `CatalogEntry::is_upgrade_over` and
`refusal_and_is_upgrade_agree_on_what_is_applyable`, saying that test is **the invariant protecting the
split**, not incidental coverage. Verified: `.is_upgrade()` is called only inside `is_upgrade_over` and in
that test; `is_upgrade_over` only in that test. **Zero production callers** — trust flows entirely through
`refusal()` — and the test really does assert `standing.refusal().is_none() == standing.is_upgrade()`
across all three variants.

### Filed, not fixed

**CPE-1984** — the catalog **rollback** route: `launcher.html:2169` renders a green
*"&lt;agent&gt; rolled back to &lt;tag&gt;."* on `r.applied > 0` with **no rejection branch at all**, and
`console.rs:1488-1497` never forwards the rejection counts, **so it structurally cannot be honest.** Same
shape as F-A, one function down. Correctly scoped out rather than widening a reviewed PR.

### Two observations recorded so they are not rediscovered as new

- The denominator **excludes `ApplyOutcome::Pinned`**, which `do_fetch_catalog` counts into none of the
  three buckets, so 1 regressed + 4 pinned renders *"1 of the 1 published agent entries"*. Pre-existing;
  this PR **strictly improves it** by adding `applied` **and deletes the old comment that over-claimed**,
  so nothing false is left at the site.
- A rejection can still mask **another rejection** (2 applied + 1 integrity + 1 regressed renders only the
  integrity sentence). Pre-existing ordering, consistent with the recorded worse-news-wins rule, and the
  acceptance criterion is specifically *not masked by a **success count***, which is met.

### One correction to the PR's own record

The consumer enumeration was **short by two** and was corrected out loud rather than silently swapped:
`broker_client.rs:329-337` **and** `:388-396` both parse the four fields as `unwrap_or(0)` passthroughs,
and `console.rs:1460 handle_catalog_rollback` re-emits a subset. Neither changes the verdict; the claim is
now *"the only reader of the **refresh response**."*

### Gates at merge

vitest **5,417 passed / 360 files** · `npm run check` **0 errors, 0 warnings** ·
`cargo clippy --all-targets --locked -D warnings` clean on `sidecar/ai-console` **and** `sidecar/host` ·
`cargo test --lib` **391** (ai-console) + **32** (host, catalog filter) · CI `completed success —
total_count=26 pending=0 skipped=1 coverage=ok`.

**"One mode, not two" verified rather than assumed:** neither sidecar crate declares `[features]`; the
repo's two-mode clippy convention is about `src-tauri`'s `sidecar-platform`, which this PR does not touch.

**Unseen, and said so:** no host rebuild, so no GUI pass — `launcher.html` is embedded in the ai-console
sidecar and a real visual check needs the **host** rebuilt, not a launcher swap. Everything verified came
from the jsdom harness driving the **real** launcher script plus source-level checks of the Rust producers.

**Family:** CPE-1924 (the `==`/`<` split this descends from, PR #1051), CPE-1984 (the rollback route's half),
CPE-1911 (the honest-status work), CPE-451 (the model snapshot), CPE-1929 (sabotage pairs, and a guard
shadowed by its producer), CPE-1932 (enumerate, don't recall), CPE-1933 (provenance claims in comments).
