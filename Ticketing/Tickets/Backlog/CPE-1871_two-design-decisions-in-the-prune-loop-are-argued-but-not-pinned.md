---
id: CPE-1871
title: two design decisions in the prune loop are argued at length and pinned by nothing
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-23
closed:
---

## Problem

Two lines in the retention path carry the longest comments in the module, and **neither is defended by a
test**. Both were found by the independent Reviewer on CPE-1863, by applying the rejected alternative and
running the full suite.

**1. CPE-1863's no-progress definition.** `snapshot_prune.rs:321` measures progress as the re-measured
store footprint falling, deliberately not as `prune` reporting bytes freed. Replace it with the rejected
form:

```rust
let progressed = freed_now > 0;
```

→ **2361 passed, 0 failed.** All five of CPE-1863's own tests pass under the definition its comment spends
a paragraph arguing against.

**2. CPE-1844's re-measure.** `store_total_bytes` is called again each iteration rather than subtracting
what `prune` reported. Apply the exact pre-CPE-1844 accounting in the current loop's shape:

```rust
let after = total.saturating_sub(freed_now);   // was: store_total_bytes(store_dir)?
```

→ **2361 passed, 0 failed.** CPE-1844's own review flagged this line as not independently red-proofable
and it was kept on argument; CPE-1863 then made it load-bearing for a second ticket without the coverage
improving.

## One test closes both

The divergence both decisions rest on has a single shape: **a blob whose last namer was pruned and whose
file could not be deleted.** `snapshot_capture.rs:738-744` credits `freed` only inside
`if fs::remove_file(&path).is_ok()`, and `:671` removes the manifest before the blob loop — so that blob
is credited **0** by `prune` and simultaneously **leaves** `total`.

Stage that, and both rejected alternatives become measurably wrong instead of merely argued against.

## Why this is not trivial, and why it was not done in CPE-1863

Staging an undeletable file defeated the Reviewer on Windows: Rust's `fs::remove_file` clears
`FILE_ATTRIBUTE_READONLY` and retries, and `icacls /deny <user>:(D)` did not block the delete in the
scratch location. Options worth trying, in rough order of portability:

- hold an open handle without `FILE_SHARE_DELETE` (Windows) — the ordinary cause in the field, since this
  is exactly the "a file is open" case
- an immutable/append-only attribute (Linux `chattr +i`, macOS `uchg`)
- a read-only **parent directory** rather than a read-only file (POSIX)
- an injectable seam on the delete, if the OS routes prove unportable — weaker evidence, but it pins the
  decision, and the module already uses injectable seams elsewhere (`tar_unpack_with`)

## Acceptance criteria

- [ ] A fixture in which `prune` reports 0 while the footprint genuinely falls. Say how you staged it and
      on which platforms it runs; skip loudly rather than silently where it cannot.
- [ ] Red-proof BOTH rejected alternatives against it — `freed_now > 0`, and
      `total.saturating_sub(freed_now)` — and record the observed failure for each.
- [ ] Assert the fixture is live: that the delete really failed and the blob really left the witness's
      answer. A fixture that quietly deleted the file would pass while proving nothing, which is the
      failure mode this whole family keeps hitting.
- [ ] If no portable staging exists, say so explicitly and record what the injectable-seam version does and
      does not prove. An honest weaker pin beats an argued-only line.

## Notes

Filed from CPE-1863's review, which approved the PR and classed both as non-blocking precisely because the
fix's *behaviour* is right — it is the *defence* that is missing. Its own summary: this "pins a design
decision that is currently argued but not defended."

Related: CPE-1863 (the no-progress rule), CPE-1844 (the re-measure), CPE-1861 (`manifests_naming`, the
witness both depend on).
