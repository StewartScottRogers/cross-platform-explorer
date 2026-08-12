---
id: CPE-1680
title: The GUI-smoke ratchet trusts its own inputs in three places — an unknown test state, an unescaped title, and an unevidenced exemption
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Three findings from the CPE-1677 gauntlet that were correctly judged non-blocking for that PR — the gate it
built is a large net improvement and none of these undoes it — but which share one root and should be fixed
together: **`evaluate()` and its reporter trust their inputs.** The ratchet is now the thing that decides
whether a GUI regression ships, so the places where it takes something on faith are worth closing.

Two of the three were found *independently* by the reviewer and the UAT, which is the reason this is a
ticket and not a shrug.

### 1. An unrecognised wdio state is silently treated as "skipped" (reviewer nit 2)

`gui-smoke/lib/ratchet.ts` maps wdio's per-test state into pass / fail / skip. Anything it does not
recognise falls through to `skipped`, and a skipped case is exempt from every clause. So if wdio ever emits
a state this code has not seen — a new version, a new runner mode, a state produced by a crash path — a
**failing** case is silently reclassified as one that never ran, and the gate stays green.

This is the same shape as the bug CPE-1677 was filed to fix: not a wrong answer, but a confident green
where the truth is unknown. The default for an unknown state must be "I don't know", and "I don't know"
about a test result has to red the run or be reported loudly — never be quietly folded into the one bucket
that means "safe to ignore".

### 2. A nested-quote rendering bug in the paste-this-in message (reviewer nit 3)

The failure message prints the JSON the user should add to `known-failing.json`. When the test title itself
contains a double quote — as wdio's own hook titles do, e.g. `"before all" hook` — the printed snippet
comes out as `<hook> ""before all" hook""`: not valid JSON, and not pasteable.

The message's whole value is that it saves log archaeology (UAT confirmed that value from a real red run),
so a case where the paste is broken defeats the feature exactly when the user is already confused. Serialise
the title through `JSON.stringify` rather than string-concatenating quotes.

### 3. `intermittent: true` has no machine-checkable bar (reviewer nit 5, and independently the UAT)

The reviewer flagged that the bar for the marker is prose in the README. The UAT then went further and
**proved** it by probing `evaluate()` directly: a case that fails on every single run — a permanent, genuine
break — is silenced forever by the field alone, with an empty `reason` and an empty `ticket`. Nothing in the
code requires the case to have ever been observed passing, which is the entire definition of intermittent.

What *does* exist, and should be preserved by any fix: every intermittent entry prints its observed status
on **every** run, and the case must still exist (clause 3 still applies), so a rename or deletion still reds
the job. It is visible; it is just not evidenced.

Today's four users of the marker are well-evidenced (eight cited run IDs) and owned by an open ticket
(CPE-1679). The risk is entirely about the next user of it.

## Scope

`gui-smoke/lib/ratchet.ts` and its reporter. Suggested directions, not prescriptions:

1. Make an unrecognised state an explicit outcome that reds the run (or at minimum names itself in the
   output and is counted), rather than defaulting into `skipped`.
2. `JSON.stringify` the test title when building the suggested entry.
3. Give `intermittent` a bar the code can check — e.g. require a non-empty `reason` citing at least two run
   references and a non-empty `ticket`. A structural check cannot prove flakiness, and should not pretend
   to; it can refuse an entry with no evidence attached at all, which is the actual failure mode.

Whatever shape (3) takes, it must not force the four current entries to be rewritten dishonestly to satisfy
a format — they already carry real evidence, so the check should pass on them as they stand.

## Acceptance criteria

- [ ] An unknown wdio state does not silently become an exempt result; a test fixture proves it.
- [ ] A test title containing a double quote produces a valid, pasteable JSON snippet in the failure
      message; a test fixture proves it with a real wdio-shaped hook title.
- [ ] An `intermittent` entry with no evidence is rejected by the gate, and the four existing entries still
      pass unchanged.
- [ ] Each of the three has a test that goes red when its fix is reverted — checked one at a time, per the
      guard-neutralisation rule.

## Notes

Filed by the Foreman from the PR #864 review (nits 2, 3, 5) and the independent UAT on the same PR,
2026-08-12. None blocked that PR and none should be read as a criticism of it — the ratchet it landed is
what made these visible in the first place.

Related: **CPE-1677** (the case-level ratchet), **CPE-1679** (the media flake, which owns the four
`intermittent` markers and is the reason item 3 matters now), and **CPE-1639** (the deliberate-break
experiment that started this chain).
