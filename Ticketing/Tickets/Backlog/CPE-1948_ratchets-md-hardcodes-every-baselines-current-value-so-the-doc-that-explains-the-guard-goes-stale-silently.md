---
id: CPE-1948
title: `RATCHETS.md`'s enumeration table hardcodes every baseline's current value, so the doc explaining the guard goes stale the moment any baseline legitimately moves
type: task
priority: Low
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

CPE-1934 (PR #1052) built the guard that makes a raised ratchet baseline loud, and documented all
twelve baselines in `docs/design/RATCHETS.md`. That table carries each baseline's **current value as
a literal**, and nothing ties those literals to the measured ones.

It went stale within an hour of landing. PR #1055 (CPE-1922) recounted the manual-test burndown from
16 to 13 — a **legitimate lowering**, exactly what a ratchet is for — and `RATCHETS.md:102` went on
saying `16`. Corrected by hand on `main` (`ratchet-baselines.mjs print` reports **13**).

**This is the family the guard itself belongs to.** CPE-1933 is filed about provenance claims in
comments; CPE-1932 about rules followed from memory rather than enumerated. This is a stored number
in the document that explains the mechanism for keeping stored numbers honest.

## Why it is Low and not Medium

Nothing enforces the table, so a stale value misleads a reader but cannot make the guard wrong — the
guard reads the real files. The cost is that the one document a person consults to understand the
system can quietly disagree with the system.

## Acceptance criteria

- [ ] **Derive the table's values rather than restating them.** Cheapest shape: a test that runs the
      registry's own measurers and asserts each row in `RATCHETS.md` matches — the same style as
      `sectionDocs.test.ts` and the CPE-1928 Rust→TS derivation guard, both of which already live in
      this repo. Generating the table is also acceptable; asserting it is probably better, because
      the prose around each row is human-written.
- [ ] **Red-proof it both ways**: change a stored value and confirm the test reds naming the row; move
      a real baseline and confirm it reds until the table is updated. A guard only ever seen passing
      is the defect this whole family is about.
- [ ] Include the **not-gated** rows. `manual-test-mvd` is enumerated but deliberately ungated, and it
      is the one that went stale first precisely because nothing gates it.
- [ ] While there: check whether any **other** row is already stale. Recount each from the registry
      rather than trusting the table (CPE-1932). Report what you find even if it is nothing.
- [ ] Consider whether the count of rows should be pinned too — a baseline added to the registry and
      not to the doc is the same defect, one level up, and the registry's completeness test already
      knows how to ask that question.

## Notes

Filed 2026-08-27 by the sprint Foreman after correcting the drift by hand. Found by the merge-order
interaction between PR #1052 and PR #1055 — flagged in advance by both PRs' authors, which is why it
was caught immediately rather than months later.

Family: **CPE-1934** (the guard this documents), **CPE-1933** (provenance claims untested by
construction), **CPE-1932** (enumerate, do not recall), **CPE-1929** (guards that cannot go red).
Related: **CPE-1922** (the legitimate lowering that exposed it).
