---
id: CPE-1954
title: `catalog-sign verify` is the one path-forming index read that does not go through `VerifiedIndex` — and it is the one input that never passes `sign_bundle`
type: bug
priority: Low
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

`sidecar/host/src/bin/catalog_sign.rs:60` parses a catalog index with `CatalogIndex::from_json`
rather than `VerifiedIndex::open`, then does `dir.join(format!("{}.json", entry.id))`. It is the only
remaining path-forming read of an index that does not go through the verifying constructor CPE-1940
introduced and CPE-1949 added the `entry.id` charset rule to.

## The instance is NOT closed by `sign_bundle`, and that is the point

The Foreman's first framing was that CPE-1949's `sign_bundle` check closes the instance and only the
*class* remains. PR #1063's worker corrected it, and the correction is the load-bearing part:

> `sign_bundle` guards what **this repo publishes**. `catalog-sign verify <dir> <pubkey>` reads a
> directory the maintainer points it at, under a key they supply on the command line — an inspected
> third-party or downloaded bundle never passes through `sign_bundle` at all. So the traversal read
> survives for exactly the input that diagnostic exists to handle.

A maintainer verifying a bundle they did not build is the whole use case, and it is the use case
still unguarded.

## Why it stayed out of PR #1063

Unchanged and still valid: `VerifiedIndex::open` folds in the schema check, so verifying a
**future-schema** bundle would return "no index" rather than a verify result. Arguably more correct,
but it is a publishing-UX call with its own error wording, and it does not belong bolted onto a
security fix.

Severity is genuinely low — read-only, maintainer-run, and its verify-then-parse **ordering is
already right**, so this is not CPE-1940's defect recurring.

## Why it is still worth doing

Closing it makes *"every path-forming read of a catalog index goes through `VerifiedIndex`"* a
**statable, guardable invariant** instead of "all but one". An invariant with one exception is one a
future reader has to rediscover, and this repo has spent a week finding the cost of that shape.

## Acceptance criteria

- [ ] Switch `catalog_sign.rs`'s verify path to `VerifiedIndex::open`.
- [ ] **Handle the schema case deliberately.** A future-schema bundle must produce a message that says
      *the schema is unsupported*, not "no index" — that regression is the only reason this was
      deferred, and shipping it would trade a small hole for a confusing diagnostic.
- [ ] **Demonstrate the traversal read first** on a third-party-shaped bundle with a hostile `entry.id`
      — the input `sign_bundle` never sees. Assert on the filesystem. If something upstream already
      constrains it, record that and close honestly.
- [ ] **Then make the invariant guardable.** Add a check that no other site parses an index and forms a
      path from `entry.id` without going through `VerifiedIndex` — the pattern CPE-1933 established
      (read the source, assert on it) is the right shape, and `workflow_scan.rs` / the shared
      `cases.json` from PR #1060 are the worked examples. Red-proof it by reintroducing a bare
      `from_json` + `join` and confirming it reds.
- [ ] Enumerate rather than recall: confirm there is no *third* site (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1063's Security Auditor (non-blocking observation) and
its worker's correction of the Foreman's framing. That PR returned **SEC PASS**.

Related: **CPE-1949** (the charset rule, PR #1063), **CPE-1940** (`VerifiedIndex` and the
verify-before-use ordering, PR #1058), **CPE-1933** (derive claims rather than assert them — the shape
the guard here should take), **CPE-308** (the catalog pipeline).
