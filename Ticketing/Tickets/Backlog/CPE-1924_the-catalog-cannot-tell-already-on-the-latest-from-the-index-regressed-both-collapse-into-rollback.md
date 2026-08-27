---
id: CPE-1924
title: the catalog cannot tell "you're already on the latest" from "the index regressed to something older" — both collapse into `Rollback`
type: bug
priority: Medium
status: Open
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
