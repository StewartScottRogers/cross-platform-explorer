---
id: CPE-1911
title: the AI Console says "Agents are already up to date" whether the catalog is current or the pipeline is dead
type: bug
priority: High
status: Doing
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

The AI Console fetches a signed agent catalog. When that fetch produces nothing new, it tells the user
one of two reassuring things — and **both are shown regardless of whether anything is actually wrong.**

Measured verbatim by CPE-1893's independent UAT, driving the real `refreshCatalog()` from
`sidecar/ai-console/src/launcher.html` through the repo's jsdom launcher harness and reading `#msg`:

| situation | what the user is told |
|---|---|
| catalog genuinely current, nothing to apply | **"Agents are already up to date."** |
| catalog a month stale, served, anti-rollback silently rejects it | **"Agents are already up to date."** — identical |
| catalog URL returns 404, publishing pipeline dead | **"No agent update available (offline, or none published yet)."** |

The first two are indistinguishable. The third is at least different, but it conflates *"you are
offline"* with *"the pipeline that publishes this has been dead for a month"* — one is the user's
problem and the other is emphatically not.

**This is live right now.** The catalog URL currently returns a hard 404: `/releases/latest/` resolves
to a sidecar release carrying no catalog asset, and neither does the latest plain tag. Confirmed with a
direct request, twice, by two independent agents.

## The cause, traced to the line

The information exists and is thrown away one layer below the UI.

`fetch_catalog_response` (`src-tauri/src/lib.rs:9965`) puts the real error string into the JSON body's
`"error"` field. `handle_catalog_refresh` (`sidecar/ai-console/src/console.rs:1342`) reads only
`res.index_ok` and `res.applied`, and rebuilds the response as
`json!({"indexOk", "applied", "agents"})` — the error never reaches the HTTP response the UI reads. So
the launcher cannot distinguish these cases even in principle; it is not a wording problem in the
front end.

**This is not a security hole and should not be triaged as one.** The same UAT confirmed the trust
engine is sound and untouched: a signature failure leaves the previous catalog standing rather than
failing open, and anti-rollback is unconditional on the normal path. The defect is that a dead pipeline
is indistinguishable from a healthy one *to the person using it*.

## Acceptance criteria

- [x] Forward the error through `handle_catalog_refresh` so the UI can tell the cases apart. The value
      already exists at `lib.rs:9965`; this is plumbing, not new detection.
- [x] Distinguish, in what the user sees: genuinely current; served but older than expected; and could
      not be fetched at all. The third should not read as reassurance.
- [x] Do not turn this into an alarm the user cannot act on. "The catalog could not be fetched" is
      honest; a stack trace or an HTTP status alone is not. Decide what a non-technical user should do
      and say that.
- [x] Red-proof each of the three states through the launcher's jsdom harness and paste the verbatim
      `#msg` text for each — that harness is how this defect was measured, and it is the cheapest way to
      prove the fix.
- [x] Leave the trust engine alone. `catalog.rs`'s signature check, content-hash binding, anti-rollback
      and last-known-good fallback are all correct; this ticket touches reporting only.

## Notes

Filed 2026-08-26 from CPE-1893's independent UAT, which passed that PR and flagged this as a separate
defect in a different component — correctly, since CPE-1893 owns the publishing job and this is the
consumer's reporting of it.

Related: **CPE-1893** (the catalog job silently starved for a month, plus the new freshness alarm),
**CPE-308** (the catalog auto-update pipeline), **CPE-1894** / **CPE-1908** / **CPE-1909** (why
`/releases/latest/` currently resolves to a sidecar release with no catalog asset — the 404's immediate
cause), [[launcher-ui-has-a-jsdom-test-harness]] (the harness that measured this).

Worth noting for whoever picks it up: CPE-1893 adds a *maintainer-facing* alarm for exactly this
condition. This ticket is the *user-facing* half. Neither replaces the other — the maintainer alarm
fires daily in CI, and the user is looking at the app right now.

## Work Log

2026-08-26 — Fixed. The plumbing gap was exactly where the ticket said: `do_fetch_catalog`
(`src-tauri/src/lib.rs`) already had everything it needed but only ever emitted `indexOk` +
`applied` + a raw rejected count; `CatalogFetch` (`sidecar/ai-console/src/broker_client.rs`) then
threw even that away, keeping only `index_ok`/`applied`. Widened the pipe instead of adding new
detection:

- `do_fetch_catalog` now also reports `staleRejected` — the count of entries that verified fine but
  were rejected by anti-rollback because they weren't newer than what's installed. This is what
  distinguishes "served but older than expected" from "genuinely nothing to apply"; both used to
  collapse to `applied == 0`.
- The existing `error` (fetch/parse failure, e.g. a 404) and `offline` (checked before any fetch was
  attempted) fields — already produced by the host — now flow through `CatalogFetch` →
  `handle_catalog_refresh`'s JSON body, instead of being read no further than `indexOk`/`applied`.
- `refreshCatalog()` in `sidecar/ai-console/src/launcher.html` now branches on all of
  `applied` / `error` / `offline` / `indexOk` / `staleRejected` to pick one of five honest messages
  instead of collapsing every non-"updated" outcome into "up to date" or one generic fallback line.
  None of them are an alarm — each says plainly what happened and, where relevant, that no action is
  needed from the user.
- Trust engine untouched: `sidecar/host/src/catalog.rs` (`apply_bundle_with`, `ApplyOutcome`,
  anti-rollback, last-known-good) has zero diff; `ApplyOutcome::Rollback` was already computed there,
  this only reads it one layer up.

Red-then-green, via the real `refreshCatalog()` driven through the launcher's jsdom harness
(`src/lib/ai-console-launcher.test.ts`, new `describe("refreshCatalog honesty (CPE-1911)")` block):
reverted `launcher.html` to the pre-fix `refreshCatalog()` body, confirmed 3 of the 4 new tests fail
(the pre-existing "genuinely current" case still passed, as expected), then restored the fix and
confirmed all 4 pass. Verbatim `#msg` text pinned by the tests:

| situation | `#msg` text |
|---|---|
| genuinely current (`indexOk:true, applied:0, staleRejected:0`) | `Agents are already up to date.` |
| served but stale (`indexOk:true, applied:0, staleRejected:2`) | `The published catalog isn't newer than what's installed, so nothing changed. If this doesn't clear up on its own, the publishing pipeline may be stuck — no action needed on your end.` |
| fetch failed, e.g. 404 (`indexOk:false, error:"fetch failed: status code 404"`) | `Couldn't check for agent updates (fetch failed: status code 404). Nothing changed — no action needed from you; try again later.` |
| offline (`indexOk:false, offline:true`) | `You're offline, so agent updates weren't checked. Nothing changed.` |

Verification: `cargo clippy -p cross-platform-explorer --features sidecar-platform --all-targets -- -D
warnings` clean, same clean without the feature; `cargo clippy --all-targets -- -D warnings` clean in
`sidecar/ai-console`; `cargo test` green in `sidecar/ai-console` (382 unit + 15 integration) and the
`sidecar/host` `catalog` module (14/14, unchanged); `npx vitest run src/lib/ai-console-launcher.test.ts`
78/78 green; full `npx vitest run` 4607/4609 green (the 2 failures are a pre-existing, unrelated
`msrvSync.test.ts` gap on `main`, not touched here); `npm run check` clean. No new deps, no
`specta::Type` structs touched, no new user-facing section (existing surface, wording fix only) so no
`src/docs/*.md` change needed.

### Round 2 (PR #1040 review — CHANGES REQUESTED)

Independent Reviewer upheld the round-1 claims (trust engine untouched, clippy clean both feature
modes, no new deps, no specta drift) but found the bug still reachable a different way, plus two
smaller gaps. All addressed on the same branch/PR:

**F1 (blocker) — fixed.** `staleRejected` only counted `ApplyOutcome::Rollback`. When the index
verifies fine but every *listed entry* fails its own integrity check (`ContentMismatch` /
`MissingManifest` / `MissingSignature` / `BadSignature` — e.g. the pipeline signs the index but
botches signing an entry), that produced `indexOk:true, applied:0, staleRejected:0`, which fell
through every branch straight into "Agents are already up to date." — the same lie, a different
trigger. Added `integrityRejected` through the same pipe (`do_fetch_catalog` → `CatalogFetch` →
`handle_catalog_refresh` → `launcher.html`) and its own honest branch: *"The published catalog looks
corrupted or mis-signed, so nothing was installed. Your existing agents are untouched and safe to use
— no action needed on your end."* `Pinned` stays excluded from both counts — user's own choice, not a
lie.

**F2 — fixed in this PR (not deferred).** `setMsg(m, ok)` only had two colours; all four "nothing
changed, no action needed" branches (error, offline, stale, integrity, plus the pre-existing
signature-invalid catch-all) painted in the same alarm red as an actionable error like "needs an API
key". Chose to fix now rather than file a follow-up: the change is small (one ternary → one
three-way branch, reusing the `#d08a1a` amber already used for `STATE_META`'s "blocked" dot elsewhere
in this file) and the round-1 fix doesn't land as *intended* without it — the words say "no action
needed" but red said the opposite, which is exactly the kind of mixed signal this ticket exists to
remove.

**Pipe now pinned end to end with Rust-side tests** (the gap the Reviewer identified: the jsdom tests
only ever fed `refreshCatalog()` a hand-built JSON blob and could never catch a field dropped in
Rust):
- `broker_client::tests::fetch_catalog_parses_the_full_rejection_and_error_breakdown` — host JSON →
  `CatalogFetch` parse, both a "rejections happened" shape and an "offline + error" shape.
- `console::tests::catalog_refresh_route_forwards_the_full_rejection_and_error_breakdown` — a
  scripted `CatalogRefreshDialogs` POSTs `/api/catalog/refresh` and asserts the response JSON carries
  `staleRejected`/`integrityRejected`/`offline`/`error` verbatim.

Red-then-green (fix committed first, then each probe reverted and restored):
- Removed the `integrityRejected` branch from `refreshCatalog()` → new JS test failed
  (`"Agents are already up to date."` instead of the corrupted/mis-signed message) → restored.
- Reverted `setMsg` to the old two-colour ternary → 4 of the 5 colour-pinning JS tests failed (green
  where amber was expected) → restored.
- Hard-coded `integrity_rejected: 0, error: None` in `BrokerDialogs::fetch_catalog` → the new
  `broker_client` test failed (`left: 0, right: 3`) → restored.
- Dropped `"offline"` from `handle_catalog_refresh`'s emitted JSON → the new `console` route test
  failed (`left: Null, right: false`) → restored.
- `git diff` confirmed byte-identical to the committed state after every restore.

Re-verification after round 2: `cargo clippy` clean in both feature modes for
`cross-platform-explorer` and in `sidecar/ai-console` (caught and fixed one new
`cloned_ref_to_slice_refs` lint from the new test); `cargo test --lib` in `sidecar/ai-console` —
384/384 (was 382, +2 new); `npx vitest run src/lib/ai-console-launcher.test.ts` — 79/79 (was 78, +1
new); `npm run check` clean; `sidecar/host/` confirmed zero-diff against `origin/main`.
