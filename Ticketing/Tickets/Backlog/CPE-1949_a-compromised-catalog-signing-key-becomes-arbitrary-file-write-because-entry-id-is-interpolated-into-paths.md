---
id: CPE-1949
title: a compromised catalog signing key becomes **arbitrary file write**, because `entry.id` is interpolated into five paths with no charset check
type: task
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Defence-in-depth finding from PR #1058's Security Auditor, raised while confirming that PR closed the
pre-verification traversal (CPE-1940 F-B) correctly.

After `VerifiedIndex` proved the index signature, `entry.id` is still interpolated into paths with no
validation: `write_entry`, plus four `staging.join(format!("{id}…"))` sites.

    signing key compromised, id = "../../.."  ->  arbitrary file write anywhere the app can reach

**Exploiting it requires the catalog signing key**, and the auditor was explicit that this makes it a
hardening item rather than a live hole — with that key an attacker can already ship an arbitrary
malicious agent through every gate, so the traversal adds *where*, not *whether*. It also judged
CPE-1940's decision to scope it out as **the right call**: fixing it inside that diff would have meant
adding a sanitiser to the path that PR had just proved does not need one, muddying a clean reordering.

## Why it is still worth doing

The blast radius is meaningfully different:

    without this: key compromise -> install a malicious agent
    with this:    key compromise -> arbitrary file write anywhere the app can reach

That is the difference between a bad agent and a compromised machine, and the mitigation the auditor
proposed is one cheap check at one place.

## Acceptance criteria

- [ ] Validate `entry.id` against a strict charset at **`VerifiedIndex::open`** — the auditor's
      suggestion is `[A-Za-z0-9._-]+`, rejecting `.` and `..` outright. One place, so it cannot be
      forgotten at a call site, and it sits where every consumer already funnels through.
- [ ] **Reject, do not sanitise.** A rejected id is a refusal the publisher can see and fix; a
      sanitised one silently writes to a path nobody chose. Refuse the whole index rather than
      dropping one entry, unless there is a reason not to — say which and why.
- [ ] **Demonstrate before and after.** With a key you control, publish an index with
      `id = "../../pwned"`, show the write landing outside the catalog dir, then show it refused.
      Assert on **the filesystem** — that the escaped location does not exist — not on a verdict.
- [ ] **Do not weaken CPE-1940's ordering.** The check must run inside `VerifiedIndex::open`, after
      signature verification, not become a new pre-verification parse.
- [ ] Confirm every real published `entry.id` passes. Read them off the live catalog, not off the
      schema (PR #1053 found this repo's assumptions about published artifact names were wrong twice).
- [ ] Red-proof: disable the check and confirm the traversal test reddens on the harm assertion.

## Also from the same audit — decide, do not silently inherit

Three measured residuals the auditor recorded without blocking. Each needs a recorded decision, not
necessarily a change:

1. **The absent-map route survives.** CPE-1940 made a *damaged* `versions.json` fail closed; **deleting**
   it still yields `applied=["claude"]` with the ancient payload written and the map rewritten to
   `{"claude":1}` — measured. Absent ⇒ first run is intentional and documented. Severity is genuinely
   low: `agents.rs:301` never consults `versions.json`, so a local attacker who can delete it can
   equally drop an old signed manifest+`.sig` straight into the catalog dir. **Decide whether the
   baseline should be anchored to something that cannot simply be removed**, and record the answer.
   Note PR #1058's body reads broader than what shipped — it closes damage, not deletion.
2. **`apply_bundle` / `apply_bundle_with` remain `pub` with no production callers**, and the latter
   still takes `&mut VersionMap` — so a future caller could reintroduce the fail-open with
   `load_versions(..).unwrap_or_default()`. Nothing guards that. Narrow the visibility, or pin it.
3. **The staging dir is `temp_dir()/cpe-catalog-stage-<pid>`** — predictable, outside the project, and
   `create_dir_all` succeeds onto a pre-existing junction. Pre-existing, untouched by #1058, and
   adjacent to the whole CPE-1896/CPE-1913 containment family.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1058's Security Auditor (SEC PASS), which enumerated
every route to `applied` and to disk, probed **31** hostile `versions.json` shapes, and compile-tested
four `VerifiedIndex` bypass attempts before raising these.

Related: **CPE-1940** (the reordering, PR #1058), **CPE-1924** (the single-comparison design),
**CPE-1941** (the publish-side route to stale content), **CPE-1896** / **CPE-1913** (the containment
family the staging-dir item belongs to).
