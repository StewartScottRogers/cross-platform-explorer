---
id: CPE-1677
title: The GUI-smoke ratchet works at spec-file granularity, so a case regressing inside an already-known-failing spec is invisible
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-12
closed:
---

## Problem

Found by the CPE-1639 worker while producing deliberate-break evidence for the font-preview settle check —
and it is a bigger finding than the ticket it came out of.

`known-failing.json` and `ratchet.ts` operate at **spec-file** granularity. `samples.smoke.ts` is already
known-failing for unrelated reasons (CPE-1507: `crypto/*`, `.eml`, `.ics`, `.vcf`, `.json`). So when the
worker deliberately broke the font case inside that spec file and ran the real GUI-smoke job, the **baseline
run and the deliberately-broken run produced the identical ratchet verdict**:

```
38 passed, 3 failed, 3 known-failing — OK
```

The job passed both times. Only the raw per-test log showed the flip. The worker had to read it by hand to
get signal at all.

## Why it matters

Every case inside a known-failing spec file is currently guarding **nothing at the gate**. It can go from
passing to failing and CI stays green, because the file was already counted as failing. That is the
difference between a test that runs and a test that guards — the distinction this crew has spent the whole
night filing tickets about, now sitting in the harness that is supposed to catch everything else.

It also quietly caps what the QA Architect's burndown can claim: a surface "covered by gui-smoke" inside a
known-failing spec is covered by observation only, not by a gate.

## Scope

Move the ratchet from file granularity to **case granularity**:

1. `known-failing.json` records individual test titles (or stable ids), not whole spec files.
2. `ratchet.ts` compares the observed per-case results against that list and fails when **a case not on the
   list fails** — which is what makes a regression inside a partially-failing file visible.
3. It should also fail when a case **on** the list starts passing without being removed, so the list drains
   rather than accumulating — the ratchet direction the burndown depends on.
4. Migrate the existing entries: `samples.smoke.ts`'s CPE-1507 set (`crypto/*`, `.eml`, `.ics`, `.vcf`,
   `.json`) becomes explicit per-case entries rather than a whole-file exemption.

Watch for the obvious trap: test titles are strings and can drift. If a listed title stops existing, that
must be a **failure** ("this exemption no longer matches anything"), not a silent pass — otherwise renaming a
test becomes a way to lose its exemption *and* its coverage at once.

## Acceptance criteria

- [ ] Deliberately breaking one case inside `samples.smoke.ts` — the exact experiment the CPE-1639 worker
      ran — **fails the job**, where today it does not.
- [ ] The CPE-1507 cases still pass the gate while genuinely failing, with each listed by name.
- [ ] A listed case that starts passing fails the gate until it is removed from the list.
- [ ] A listed title that matches no test fails the gate.
- [ ] The whole thing is proven by running the real job on GitHub Actions, not by unit tests of the ratchet
      alone — with the run URLs for the break and its revert.

## Notes

Filed by the Foreman from the CPE-1639 work, 2026-08-12. The worker flagged it as an observation in that
ticket's Work Log and deliberately did not fix it unilaterally, which was the right call — it is a change to
the shape of the gate, not a fix to the ticket it was found under.

Evidence to reuse: the three real GUI-smoke runs from that work — baseline `31593963928`, deliberate break
`31598207125`, reverted final `31602466829`. The break run is the one whose per-test log shows
`✖ opens samples/fonts/mini.ttf` while the job-level verdict stayed "OK".
