---
id: CPE-1947
title: a "could not check" refusal leaks the path into the sentence, and suppresses a real link explanation entirely — including, now, its remedy
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Two pre-existing defects in the macro run-confirm dialog's collision explanations, found by PR
#1056's independent Reviewer. Both predate CPE-1928 and are unchanged by it — the Reviewer measured
that explicitly and did not block on them — but the second is **marginally sharper** after that PR.

## F-A: the "could not check" arms leak the path into the sentence

`crates/server/src/fsutil.rs` has arms that refuse because the guard could not determine whether a
destination is a link, ending *"…refusing to guess rather than risk …: {e}"*. Those messages open

    could not check whether "<path>" is a link

and `genericizeReason`'s strip regex is `/^"[^"]*"\s*/` — **anchored**, so it cannot match a message
that opens with `could`. The path therefore renders in the sentence.

CPE-1891 established that this dialog must not leak the path into its prose; the path belongs in the
list below, rendered through `displaySafePath`. That property is enforced for the link-hazard arms
(and CPE-1928 strengthened it to a direct `not.toContain()` against every fixture path on every
sentence) — but the "could not check" arms were never covered, on either side.

## F-B: bucket dedup lets a "could not check" refusal suppress a real link explanation

`representativeReasons` deduplicates by bucket. If a "could not check" **rename** collision appears
before a real-link **rename** collision, the real one is suppressed entirely — the user is told the
guard could not check, and never told that a destination is genuinely a link.

**After CPE-1928 this loses the remedy as well as the sentence.** The shared remedy
(*"remove the link first if that is what you meant"*) is hoisted out and rendered once, driven by
`sawLinkHazard`; a suppressed link hazard no longer contributes it. Before that PR the whole
explanation was already being dropped, so this is a sharpening rather than a new defect — but it is
the actionable half that is now missing.

## Acceptance criteria

- [ ] Make `genericizeReason` handle the "could not check" opening, or give those arms a message
      shape the existing strip can match. Prefer changing the **backend message** to lead with the
      same `"<path>" is …` shape the other arms use, so one strip rule covers every arm rather than
      accumulating special cases.
- [ ] **Pin the no-path property for these arms too** — CPE-1891's rule applies to every sentence the
      dialog renders, not just the ones that happened to have tests. Assert `not.toContain()` against
      the fixture path, the same way CPE-1928 does for the link arms.
- [ ] Fix the dedup so a "could not check" refusal cannot suppress a genuine link hazard. Either
      bucket them separately, or prefer the more informative refusal when both are present in one
      bucket — decide and record which, because "we could not check" and "it is definitely a link"
      are different facts and the second is strictly more useful.
- [ ] **The remedy must survive.** Whatever the dedup does, if any collision in the run is a real link
      hazard the user must be told to remove the link. Assert that directly.
- [ ] Extend CPE-1928's Rust→TS **derivation guard** to cover the "could not check" arms as well.
      That guard reads `fsutil.rs`, walks the `format!` literal out, and asserts byte-identity against
      the fixtures — it currently covers three messages. Arms outside it can drift silently, which is
      how both of these went untested on **both** sides.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1056's Reviewer, which found both while verifying
CPE-1928 and correctly scoped them out (either fix means revisiting how a non-link-shaped refusal is
bucketed at all, which is past that ticket's XS).

Related: **CPE-1891** (the no-path property and the collision dialog), **CPE-1928** (the prose split
and the derivation guard, PR #1056), **CPE-1933** (a frontend parse of a backend string with nothing
binding them — the shape the derivation guard exists to close).
