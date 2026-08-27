---
id: CPE-1940
title: a corrupt `versions.json` makes anti-rollback fail **open**, and an unverified attacker-controlled `entry.id` reaches a fetch URL and a staging path before `verify_index` runs
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Two pre-existing routes through the agent-catalog trust engine, found by PR #1051's independent
Security Auditor while proving CPE-1924 safe. **Both are outside that diff** — the auditor verified
that explicitly and passed the PR. The first is **measured**; the second is **inferred from reading,
not executed**, and this ticket keeps that distinction rather than laundering it.

## F-A (MEASURED): `load_versions` is fail-open, so a corrupt map re-applies an ancient bundle

`sidecar/host/src/catalog.rs:190` — a corrupt or missing `versions.json` yields an **empty map**. Every
entry then reads `installed = None`, which is `VersionStanding::Newer`, which is `Accept`.

The auditor ran the same ancient v1 bundle twice:

    intact map          -> refused
    corrupt-derived map -> APPLIED, and the version map pushed BACKWARDS to v1

So the whole anti-rollback rule is defeated by damaging one local file — no signing key needed, no
network position. The engine's fail direction here is **open**, and everything else in this subsystem
(index signature, manifest signature, content hash) fails **closed**. That inconsistency is the bug.

Note this is the third distinct route by which catalog content can go backwards, alongside CPE-1941.

## F-B (INFERRED — read, not executed): `entry.id` is used before the index is verified

In `do_fetch_catalog` (`src-tauri/src/lib.rs`, around 10312-10320) the index is parsed and each
attacker-controlled `entry.id` is interpolated into **both** a fetch URL and a staging path:

    staging.join(format!("{}.json", entry.id))

`verify_index` runs **later**, inside `apply_bundle_with`. So an `id` containing `../` would traverse
before anything has established the index is trustworthy at all. This **inverts the verify-before-use
rule** the rest of the subsystem follows.

Reachability is limited: it needs the ability to serve the catalog assets, i.e. a TLS or GitHub
compromise, so it is not reachable in normal operation. That is a reason to rank it below F-A, not a
reason to leave it — parsing and *using* unverified input is exactly the shape that turns a
second-order compromise into a first-order one.

## Acceptance criteria

- [x] Make `load_versions` fail **closed**: a `versions.json` that exists but cannot be parsed must
      refuse the whole apply, not silently present an empty map. Distinguish "absent, first run"
      (legitimately empty) from "present and corrupt" (a refusal) — they are different facts and
      collapsing them is the defect.
- [x] **Red-proof it the way the auditor found it**: corrupt the map, run an ancient bundle, and
      assert the apply is refused *and* the map is unchanged on disk. Assert on the **filesystem and
      the map**, not on a verdict enum.
- [x] Move the `entry.id` use behind `verify_index`, or sanitise the id before it touches a URL or a
      path — and prefer the reordering, because sanitising leaves the verify-before-use inversion in
      place for the next field someone adds.
- [x] **Demonstrate F-B before fixing it.** It is currently inferred. Build the traversal against a
      local fixture index and show where the bytes land; if it turns out something upstream already
      constrains `entry.id`, record that instead and close the finding honestly.
- [x] Check whether any other field off the unverified index is used before `verify_index` — do not
      fix only the one the auditor happened to read. Enumerate rather than recall (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1051's Security Auditor, which enumerated the trust
funnel (one `Accept` construction site, one `write_entry` caller, one `applied.push`), attacked it
with 12 probes and 2 sabotages, returned **SEC PASS** on the PR, and raised these as pre-existing.

Related: **CPE-1924** (the `==`/`<` split, PR #1051), **CPE-1941** (the third backwards route),
**CPE-1939** (the reporting residuals from the same review), **CPE-308** (the catalog pipeline).

## Work Log

**2026-08-27 — reproduced both findings, then fixed both.**

**F-A reproduced first, on the pre-fix code.** A red-proof test staged a properly signed but
**ancient** v1 bundle against an installed v9, then damaged `versions.json`. Printed damage:

    applied              = ["claude"]
    out/claude.json      = {"id":"claude","v":"ANCIENT"}     (was GOOD-v9)
    versions.json (disk) = {"claude":1}                      (was {"claude":9})

Exactly the auditor's result: one damaged local file, no signing key, no network position, and the
on-disk anti-rollback baseline is pushed **backwards from 9 to 1**.

**F-B demonstrated, not merely inferred.** Nothing upstream constrains `entry.id` —
`CatalogIndex::from_json` is plain serde over a free-form `String`. A fixture index carrying
`"id": "../../pwned"` with a garbage signature, run through the pre-fix order, put `../../` into the
fetch URL **and** wrote bytes to `<staging>/../../pwned.json`, which landed two directories above
staging. The finding stands; it is not disproved.

**Fixes.**

- `load_versions` now returns `Result<VersionMap, VersionMapError>`. *Absent* ⇒ `Ok(empty)` (a real
  first run). *Present but corrupt or unreadable* ⇒ `Err(Corrupt | Unreadable)` — the baseline is
  **unknown**, not empty. Deliberately not self-healing: a corrupt map is left byte-for-byte as
  found so the failure keeps refusing until an operator resolves it.
- New `apply_bundle_at` owns the load → apply → save cycle, so the baseline can only be read
  fail-closed. It does **not** decide applyability — `VersionStanding::refusal()` remains the single
  version comparison and the single enforcement point (CPE-1924's design is untouched, and
  `is_upgrade_over` / `is_upgrade` are left in place for its invariant test). It answers the earlier
  question: is the baseline knowable at all?
- New `VerifiedIndex` newtype whose only constructor verifies the detached signature **before** it
  parses, then checks the schema. `do_fetch_catalog` and `apply_bundle_with` both go through it, so
  an unverified index yields no entries and there is no `id` to reach a URL or a path. A reordering,
  not a sanitiser — the next field someone reads off the index is behind the same gate for free.

**Enumeration (searched, not recalled).** Grepped every `CatalogIndex`/`CatalogEntry` field name
(`id`, `schema_version`, `sha256`, `version`, `entries`) across all Rust outside `catalog.rs`. Two
production consumers exist. `do_fetch_catalog` used two things pre-verify — `entry.id` (2 URLs, 2
staging paths) and `index.entries` itself, whose length drove an unbounded fetch loop; `sha256`,
`version`, `entry.schema_version` and `index.schema_version` were **not** used pre-verify. Both are
now post-verify. `sidecar/host/src/bin/catalog_sign.rs`'s `verify` already had the correct order
(verify → parse → use `id`); left as-is and recorded rather than changed.

**Sabotage.** (A) reverting `load_versions` to fail-open ⇒ **1** test red. (B) skipping the signature
check in `VerifiedIndex::open` ⇒ **2** tests red, one of them the pre-existing
`a_bad_index_signature_touches_nothing_last_known_good`, confirming the reorder did not weaken the
existing index-signature guard.

**Verification.** `sidecar/host`: 112 pass, clippy `--locked --all-targets -D warnings` clean.
`src-tauri`: 285 pass, clippy clean in **both** feature modes (plain and `sidecar-platform`).
`src/` changes are `src/docs/04-ai-console.md` and `src/lib/ai-console-launcher.test.ts`, so
`npm run check` was run (0 errors, 0 warnings). No dependency or `specta::Type` changes.

**2026-08-27, review round 2 — Security Auditor SEC PASS; Reviewer CHANGES REQUESTED.**

**Blocking, and it was the same defect on the sibling route.** The flag was threaded through the
**refresh** route only. `/api/catalog/rollback` runs the same `host.fetch_catalog` → the same
`do_fetch_catalog` → the same `apply_bundle_at`, so a corrupt map refuses it identically — but
`handle_catalog_rollback` emitted only `{indexOk, applied, tag, agents}`, dropping both
`versionMapUnreadable` and `error`. Measured through a stub, the refusal arrived as
`{"agents":12,"applied":0,"indexOk":false,"tag":"v0.1.0"}`, which `applyRollback` rendered in the
**green success colour** as *"Nothing changed (that version may not include this agent)"* — blaming
the published version for a fault on the user's own machine, with no recovery step.

And it is a state **this change creates**: pre-fix, a corrupt map made rollback silently *succeed*
(`allow_downgrade` + an empty baseline ⇒ `Newer` ⇒ `Accept`), so there was no misleading refusal to
report before. Fixed, and pinned at all four hops — host JSON (shared), the broker parse
(`rollback_catalog_parses_the_local_baseline_refusal_too`), the console route
(`catalog_rollback_route_forwards_the_local_baseline_refusal_too`), and the launcher branch, which
is checked **before** the `applied > 0` / else pair.

**Also taken:**

- The F-A test's filesystem assertions ran *after* `expect_err`, so under the regression they guard
  the panic came from the missing `Err` and the two `fs::read` comparisons never evaluated. Reordered
  so the on-disk facts are asserted first. Re-verified under sabotage: the failure is now
  `the installed manifest was overwritten with ancient content`, printing `ANCIENT` against
  `GOOD-v9` — the damage is the thing that reddens.
- `apply_bundle_with` is now `pub(crate)`, closing the door on a future caller writing
  `load_versions(p).unwrap_or_default()` + `apply_bundle_with`. `apply_bundle` had the identical
  hazard and identical exposure, so closing only one would have been a half-fix; it had no production
  callers at all, so it moved into the test module as a helper (zero test-body churn).
- **"Reset to the shipped agents" cannot clear the `Unreadable` case it was offered for** —
  `handle_catalog_reset` uses `remove_file`, which fails on the directory-in-place shape that
  `an_unreadable_version_map_is_a_refusal_too` constructs. Rather than widen a reset handler that
  recursively deletes inside the catalog dir, both messages and the docs now say the reset clears the
  usual case and name deleting the file by hand as the fallback — no universal-sounding cure for a
  state it cannot always clear.
- Docs wording: it refuses the **apply**, not "the whole check" — the download already happened.

Deferred to **CPE-1949** (filed by the Foreman): sanitising `entry.id` post-verify, the absent-map
route, the `pub` visibility sweep, and the predictable staging dir.

## Closed 2026-08-27 — merged as PR #1058, after two rounds

**Reviewer APPROVE + Security Auditor SEC PASS.**

**F-A shipped.** `load_versions -> Result<VersionMap, VersionMapError>`: absent ⇒ `Ok(empty)` (a genuine
first run), present-but-corrupt/unreadable ⇒ `Err`. A new `apply_bundle_at` owns load/apply/save, so a
refused run **cannot** rewrite the map — enforced by construction (`?` bails before both the apply and
the save), not by a caller remembering. Deliberately **not** self-healing.

Pre-fix state, reproduced independently by the Reviewer:

    out/claude.json = {"id":"claude","v":"ANCIENT"}   (was GOOD-v9)
    versions.json   = {"claude":1}                    (was {"claude":9})

**F-B was demonstrated, not left inferred.** Nothing upstream constrains `entry.id` — `from_json` is
plain serde over a free-form `String`. With `"id": "../../pwned"` and a garbage signature, the write
landed **two directories above staging**, before `verify_index` ran. Fixed by **reordering**, not
sanitising: a `VerifiedIndex` newtype whose only constructor verifies first, so an unverified index
yields no fields to misuse. The Auditor **compile-tested** four bypasses — `E0423` on the tuple
constructor, `E0277` on `Default`/`From`/`Deserialize` — and confirmed `open` calls the *same*
`verify_index`, not a weaker re-implementation.

**The enumeration found a second pre-verify use nobody had named:** `index.entries`'s length drove an
unbounded fetch loop before verification. Both Reviewer and Auditor re-derived the field list
independently and found **no third**.

**31 hostile `versions.json` shapes** all returned `Err` — zero-length, truncated, wrong value types,
negative, float, overflow, `NaN`, BOM-prefixed, non-UTF-8, NUL-padded, a **directory** at the path, a
file **locked by another process**. Two land in "absent" and were correctly judged non-escalations: a
dangling symlink (you cannot link over an existing file, so the attacker must delete the real one
first — and deletion alone already gives the empty baseline) and duplicate keys (anyone who can write
that file can write `{"claude":1}` directly).

### The blocker was the fix's own defect, one route over

Round 1 threaded the new `versionMapUnreadable` flag through the **refresh** route only.
`/api/catalog/rollback` runs the identical `do_fetch_catalog` → `apply_bundle_at` and refuses
identically, but `handle_catalog_rollback` dropped both the flag **and** `error`. Measured:

    {"agents":12,"applied":0,"indexOk":false,"tag":"v0.1.0"}
    -> launcher renders GREEN: "Nothing changed (that version may not include this agent)."

Blaming the published version for a fault on the user's own machine, with no recovery step, forever.
And it is a state **this PR created** — pre-fix, a corrupt map made rollback silently *succeed*
(`allow_downgrade` + empty baseline ⇒ `Newer` ⇒ `Accept`).

The author's own diagnosis is the durable part: *"I fixed the reporting on refresh and never asked
which other callers reach the same code — the same 'fix what I read, don't enumerate' failure the
ticket's last AC exists to prevent."* Now pinned at all four hops, with the branch checked **before**
the `applied > 0` / else pair. The Reviewer verified it by **rendering** — amber, naming the recovery
step — rather than by confirming the JSON carries a field.

Two smaller ones worth carrying: the F-A test's filesystem assertions **never fired** under the
regression they guarded (the `expect_err` panicked first), now reordered so on-disk damage is what
reddens; and `apply_bundle`/`apply_bundle_with` went `pub(crate)`/test-helper — the author extended
that past the literal ask because `apply_bundle` carried the identical `&mut VersionMap` hazard, and
**flagged the extension rather than doing it silently.**

Residuals filed as **CPE-1949**: `entry.id` still interpolated into five paths post-verification (key
compromise ⇒ arbitrary file write); the absent-map route; the predictable staging dir.
