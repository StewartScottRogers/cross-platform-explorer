---
id: CPE-1940
title: a corrupt `versions.json` makes anti-rollback fail **open**, and an unverified attacker-controlled `entry.id` reaches a fetch URL and a staging path before `verify_index` runs
type: bug
priority: High
status: Open
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

- [ ] Make `load_versions` fail **closed**: a `versions.json` that exists but cannot be parsed must
      refuse the whole apply, not silently present an empty map. Distinguish "absent, first run"
      (legitimately empty) from "present and corrupt" (a refusal) — they are different facts and
      collapsing them is the defect.
- [ ] **Red-proof it the way the auditor found it**: corrupt the map, run an ancient bundle, and
      assert the apply is refused *and* the map is unchanged on disk. Assert on the **filesystem and
      the map**, not on a verdict enum.
- [ ] Move the `entry.id` use behind `verify_index`, or sanitise the id before it touches a URL or a
      path — and prefer the reordering, because sanitising leaves the verify-before-use inversion in
      place for the next field someone adds.
- [ ] **Demonstrate F-B before fixing it.** It is currently inferred. Build the traversal against a
      local fixture index and show where the bytes land; if it turns out something upstream already
      constrains `entry.id`, record that instead and close the finding honestly.
- [ ] Check whether any other field off the unverified index is used before `verify_index` — do not
      fix only the one the auditor happened to read. Enumerate rather than recall (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1051's Security Auditor, which enumerated the trust
funnel (one `Accept` construction site, one `write_entry` caller, one `applied.push`), attacked it
with 12 probes and 2 sabotages, returned **SEC PASS** on the PR, and raised these as pre-existing.

Related: **CPE-1924** (the `==`/`<` split, PR #1051), **CPE-1941** (the third backwards route),
**CPE-1939** (the reporting residuals from the same review), **CPE-308** (the catalog pipeline).
