---
id: CPE-1947
title: a "could not check" refusal leaks the path into the sentence, and suppresses a real link explanation entirely — remedy included
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
that explicitly and did not block on them.

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

**This is NOT sharpened by CPE-1928 — it is exactly as bad as it always was.** That PR's author and
the Foreman both believed the hoisted remedy made the suppression worse; PR #1056's Reviewer measured
every suppression ordering on both trees and disproved it:

    rename-unknown before rename-link (same bucket)  remedy rendered 0x pre-PR, 0x post-PR
    convert-unknown before convert-link              remedy rendered 0x pre-PR, 0x post-PR
    rename-link before rename-unknown                remedy rendered 1x both
    suppression + a live link in the other bucket    remedy rendered 1x both

The *mechanism* did change — `sawLinkHazard` is now computed from the bucket's **representative**
only, so a suppressed link hazard can no longer raise the shared remedy line. The *outcome* did not,
because on `main` the suppressed link message carried its remedy **inside itself** and was dropped
along with it. In these cases the representative is the `could not check` arm, which never had a
remedy clause at all.

**So do not go looking for a regression here.** The defect is that the suppression drops the entire
link explanation, remedy included, and it did so before CPE-1928 too.

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

## One-line hardening for the derivation guard, from the same review

PR #1056's Reviewer attacked CPE-1928's `format!`-literal walker with ten adversarial sources. Nine
failed **loudly** — `{{ }}` escapes, a `//` between `format!(` and the literal, reordered match arms,
a raw string, a second earlier `format!` in the same fn, and a renamed or missing fn (which throws at
**collection**, so the suite cannot run at all, let alone pass with zero comparisons).

**One constructible silent pass survives.** If a comment sits between `pub fn classify_symlink_slot`
and the real `format!(`, contains the text `format!(`, and quotes the *old* message — as a block
comment or a single `//` line — the walker anchors on the comment and pins **that** while the shipped
literal has drifted. It needs a self-inflicted in-body "this used to read `format!(…)`" comment,
which does not exist today. (Doc comments *above* the fn are safe: the scan starts at the signature.
Multi-line `///`/`//` mostly self-defeat because the per-line prefix leaks into the derived string
and reds — luck, not design.)

**The fix is one line**, and the Reviewer measured it as strictly better on every probe including the
arm reorder — anchor on the arm rather than on "the first `format!` after the fn":

    const fmt = src.indexOf("Ok(true) => Some(format!(", fnStart);

Take it when extending the guard to the `could not check` arms.
