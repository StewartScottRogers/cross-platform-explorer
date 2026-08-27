---
id: CPE-1924
title: the catalog cannot tell "you're already on the latest" from "the index regressed to something older" — both collapse into `Rollback`
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

`sidecar/host/src/catalog.rs:64-66` decides an entry is an upgrade with
`installed.is_none_or(|v| self.version > v)` — **strictly greater**. So anything not strictly newer
becomes `EntryVerdict::Rollback`, and the repo's own test at `catalog.rs:354` states it outright:
*"Same version as installed → not an upgrade → rollback attempt"*.

That merges two states a user experiences completely differently:

| real situation | `==` or `<` | what it means |
|---|---|---|
| you already have the latest published release | `==` | **healthy**, and the single most common outcome of clicking "check for updates" |
| the served index has gone **backwards** — an older catalog is being published | `<` | **genuinely broken**; the publishing pipeline regressed |

Nothing downstream can separate them, because both arrive as `Rollback`.

## Why the healthy case dominates

`.github/workflows/release.yml:402` sets `VERSION=$(date +%s)` — a fresh Unix timestamp per release
run, stamped uniformly across every entry. So the moment a user applies release R, every subsequent
check before release R+1 has `entry.version == installed` for **every** agent, and every entry is
`Rollback`. Under this versioning scheme the `==` case will vastly outnumber the `<` case in normal
operation.

## What this cost, concretely

CPE-1911 built an honest "the published catalog isn't newer than what's installed" message on top of
`staleRejected` (its count of `Rollback` outcomes) and had it warn that the publishing pipeline might
be stuck. Because of this defect that warning would have fired on **essentially every routine
check**, and the reassuring "Agents are already up to date." branch would have been close to
unreachable for a non-empty catalog. Found by PR #1040's independent Reviewer on round 2, after two
rounds in which nobody had asked *when is this signal actually nonzero*.

CPE-1911 shipped the small half of the fix — wording that is true in both cases and diagnoses
neither. **This ticket is the real fix.**

## Acceptance criteria

- [ ] Distinguish `==` from `<` at the verdict level — e.g. an `AlreadyCurrent` outcome alongside
      `Rollback` — and carry it through `ApplyOutcome` and the existing report pipe to the AI
      Console, which already has the plumbing (`do_fetch_catalog` → `CatalogFetch` →
      `handle_catalog_refresh` → `refreshCatalog`) from CPE-1911.
- [ ] **Do not weaken anti-rollback.** Both outcomes must still land in `report.rejected`, never in
      `applied`. This is a *reporting* refinement, not a trust change, and it must be provably so.
- [ ] Restore the honest split in the console: `==` reads as the calm, routine "nothing new"; `<`
      says plainly that the published catalog has **gone backwards**, which is the one case where
      "the publishing pipeline is broken" is a true statement. Update CPE-1911's wording and its
      colour treatment accordingly, and remove the code comment CPE-1911 left pointing here.
- [ ] Pin both directions with tests that go red when broken — a same-version fetch and an
      older-version fetch must produce different, asserted outcomes.
- [ ] **This touches the trust engine, so it needs a security review of its own.** `sidecar/host/`
      being zero-diff was an explicitly reviewed property of PR #1040; this ticket gives that up
      deliberately and must earn it back.
- [ ] While in there: consider whether `VERSION=$(date +%s)` is the right versioning scheme for
      catalog entries at all, or whether it is what makes every entry churn on every release. If
      changing it is out of scope, say why.

## Notes

Filed 2026-08-27 by the sprint Foreman. Deliberately scoped **out** of CPE-1911 / PR #1040 so a
trust-engine diff gets its own review rather than being bolted onto a third round.

Related: **CPE-1911** (the honest-status work that surfaced this), **CPE-308** (the catalog
auto-update pipeline), **CPE-1873** (updater pinning).

## Work Log

**2026-08-27 — picked up, moved Backlog → Doing.**

**Design decision: one comparison, two reasons.** The security-critical constraint is that the
`==`/`<` split must not create a second route by which a not-strictly-newer entry gets applied. So
the "is this an upgrade?" question is answered in exactly **one** place —
`CatalogEntry::version_standing(installed) -> VersionStanding {Newer|Same|Older}` — and *everything*
else is derived from it:

- `VersionStanding::is_upgrade()` — the trust rule (`Newer` only).
- `VersionStanding::refusal() -> Option<EntryVerdict>` — the reporting map; `None` for `Newer`,
  `Some(AlreadyCurrent)` for `Same`, `Some(Rollback)` for `Older`.
- `CatalogEntry::is_upgrade_over()` — now `version_standing(..).is_upgrade()`, not its own `>`.
- `gate_manifest_opt` — `if !allow_downgrade { if let Some(r) = standing.refusal() { return r } }`,
  so there is a single `None` arm in the whole engine and it is the only route to `Accept`.

A test (`refusal_and_is_upgrade_agree_on_what_is_applyable`) asserts
`refusal().is_none() == is_upgrade()` for every standing, so the two views can never drift apart.

**Assumption recorded:** `Same` still counts as a *rejection*, not a no-op. It is pushed to
`report.rejected` with `ApplyOutcome::AlreadyCurrent`, never to `applied`, and
`apply_reports_already_current_and_a_regressed_publish_separately_and_applies_neither` asserts
`applied.is_empty()` on that leg, that the installed bytes are untouched, and that the version map
is unchanged. Anti-rollback behaviour is byte-for-byte what it was; only the label changed.

**Reporting pipe.** `do_fetch_catalog` (src-tauri) replaces the single ambiguous `staleRejected`
count with `alreadyCurrent` + `regressedRejected`; `CatalogFetch` (broker_client.rs) and
`handle_catalog_refresh` (console.rs) carry both; `refreshCatalog()` (launcher.html) branches on
them. `staleRejected` was deliberately **removed** rather than kept as a sum — leaving a field whose
meaning is "one of two very different things" is the defect this ticket exists to remove, and the
host + sidecar ship in the same installer so there is no version-skew consumer.

The regressed branch is checked **before** already-current, so a mixed publish surfaces the
regression rather than the reassuring half (pinned by a jsdom test).

**Console wording/colour (replacing CPE-1911's non-diagnostic compromise):**
- `alreadyCurrent > 0` → "You already have the latest published agents — nothing new to install."
  in the calm/green treatment.
- `regressedRejected > 0` → "Heads up: the published agent catalog has gone backwards …" in amber.
- The CPE-1911 comment pointing at this ticket is gone (it was the only in-code reference; grepped).

**`VERSION=$(date +%s)` (last acceptance item): left as-is, deliberately.** Full reasoning in the PR
body. Short version: the timestamp is uniform-per-run, which does mean every entry's version churns
on every release even when its manifest bytes are identical — but the cost of that churn is a few KB
of re-fetched JSON, while the *user-visible* damage was entirely the `==`/`<` conflation this ticket
fixes. The alternative (carry a per-entry version forward when the manifest sha256 is unchanged)
requires the release signer to fetch and trust the previously published index at publish time — new
network and new trust surface in the release pipeline, for a cosmetic win. Not worth bundling into a
trust-engine change that is trying to stay small and reviewable.

**2026-08-27 — post-review polish (PR #1051 gated: Reviewer APPROVE, Security Auditor SEC PASS, UAT
PASS).** Rebased on `main` (#1039, #1048, #1049 had landed). Three changes:

1. **Provenance claim corrected (Security Auditor, CPE-1933 shape).** Its sabotage C — permitting
   `Same` in `is_upgrade()` while leaving `refusal()` correct — left every behavioural probe green,
   which empirically proves `is_upgrade()` / `is_upgrade_over()` are **off** the enforcement path.
   The doc comment calling `is_upgrade` "the anti-rollback rule itself" was therefore a false
   provenance claim; both predicates now say plainly that they are derived and that
   `VersionStanding::refusal` is the enforced rule, and `refusal`'s own comment says so from its
   side. The invariant test is what keeps the derived pair honest — noted in the comment.
2. **The regression sentence is now scoped by count (UAT).** With `alreadyCurrent: 3,
   regressedRejected: 1` the old wording claimed a wholesale regression. It now reads "1 of the 4
   published agent entries is older than the version you already have…" (singular/plural handled),
   with the denominator summing the rejection buckets. Priority is unchanged — the regression still
   wins over already-current. `integrityRejected` is included in the sum although it is always 0 on
   this path (its branch is checked first), so the count survives a future re-ordering.
3. **Docs finished.** `src/docs/04-ai-console.md` now covers the success and mixed-publish cases as
   well as the four "nothing changed" ones, and closes the loop for someone looking at the amber
   bar: nothing to fix on their machine, it clears when the next good catalog is published.

Not touched, deliberately: the `applied > 0` masking (filed as **CPE-1939** together with the
`integrityRejected` case) and the release-versioning scheme (filed as **CPE-1941**, whose framing —
derive `VERSION` from the tag's commit timestamp or a committed counter, needing no new network or
trust surface — is a better option than the one this ticket weighed and rejected).

Red-proof for the new wording: replaced the count-bearing sentence with an unqualified one; the two
jsdom regression tests went red on `/2 of the 2 published agent entries are…/` and `/1 of the 4
published agent entries is…/`, then restored to green.

## Closed 2026-08-27 — merged as PR #1051

Three independent gates: Reviewer **APPROVE**, Security Auditor **SEC PASS**, UAT **PASS**.

**What shipped.** `CatalogEntry::version_standing() -> VersionStanding {Newer|Same|Older}` is now the
**only** version comparison in `sidecar/host` — the Reviewer grepped the crate to confirm it, and the
only other hits are `schema_version` bounds, a different field. `is_upgrade()` and `refusal()` are
both derived from it, `refusal()` has exactly one `None` arm (`Newer`), and an invariant test asserts
`refusal().is_none() == is_upgrade()` for every standing. `AlreadyCurrent` joins `Rollback`; both land
in `report.rejected` and neither can reach `applied`.

**Anti-rollback was not weakened, and this was proved rather than argued.** The Security Auditor
enumerated the trust funnel by closed grep — `EntryVerdict::Accept` is constructed at **exactly one**
non-test line, `write_entry` has **exactly one** caller, `report.applied.push` occurs at **exactly
one** line — then attacked it with 12 hostile-catalog probes and 2 sabotages. Same-version-different-
content: rejected, disk byte-identical, no `.sig` written. Malformed/absent/negative/overflow version:
the whole index refused, fail-closed.

**Two things came out of the review that neither the ticket nor the author knew.**

1. **The UAT corrected the premise.** The Foreman's brief said `main` fired a false "pipeline may be
   stuck" warning on `<`. It checked `origin/main` and found the shipped CPE-1911 compromise showed
   *"Nothing newer than what you have is currently published…"* in **calm green** for both `==` and
   `<`. So a genuinely backwards catalog was reported as fine, with no heads-up at all. The gain is
   larger than the ticket claimed, and in the opposite direction.
2. **`is_upgrade()` is off the enforcement path.** The Auditor's sabotage C permitted `Same` in
   `is_upgrade()` while leaving `refusal()` correct — **every behavioural probe stayed green**. So the
   enforced rule is `refusal()`, and `is_upgrade`/`is_upgrade_over` now have **zero production
   callers**, kept alive only by the invariant test. **A dead-code sweep must not delete them** — that
   would take the invariant test with them. The doc comment calling `is_upgrade` "the anti-rollback
   rule itself" was corrected.

**User-visible result**, measured through the real `launcher.html` in jsdom across 13 states:
`==` → green *"You already have the latest published agents — nothing new to install."*; `<` → amber
*"…has gone backwards — 1 of the 4 published agent entries is older than the version you already
have…"*, with the count carried so it stops over-claiming a wholesale regression, and the regression
winning over already-current on a mixed publish.

**`VERSION=$(date +%s)` was deliberately left alone**, and the reasoning was corrected in review: the
fix does **not** need `catalog-sign` to fetch and trust the previous index — deriving the version from
the tag's commit timestamp or a repo-committed counter closes it with no new network or trust surface.
Filed as **CPE-1941** (High) with that framing rather than the author's more expensive one.

Residuals filed: **CPE-1939** (a regression still hidden behind "Updated 1 agent."; the model snapshot
carries the same conflation), **CPE-1940** (High — `load_versions` fail-open, measured; an unverified
`entry.id` reaching a URL and a path, inferred), **CPE-1941** (High).
