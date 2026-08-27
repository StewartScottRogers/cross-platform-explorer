---
id: CPE-1928
title: the two link-hazard sentences read as duplicates, because the only differing clause is buried mid-sentence
type: task
priority: Low
status: In Progress
tags: ready
estimate: XS
created: 2026-08-27
---

## Summary

When a macro run hits **both** link-hazard kinds at once, the macro run-confirm dialog shows two
explanation sentences that open identically ("This destination is a link, and…") and close
identically ("…remove the link first if that is what you meant"), differing only in the middle
clause. That puts roughly seven lines of near-duplicate prose above a three-line list, with the
differentiator buried where the eye does not land. At a glance it reads as the same paragraph twice.

Observed by PR #1044's Visual Critic in `.claude/sprint-metrics/visual-evidence/cpe-1891-light-many-blocked.png`
(three blocked collisions across two hazard kinds).

## Decision taken, and why it is not in CPE-1891

The Critic offered this as a taste call. **The Foreman took the better option rather than queuing a
third question for the user, and deferred it here rather than spending a fifth round on CPE-1891**,
which was otherwise finished and holding CI capacity for four sibling PRs. The condition is
uncommon — it needs one run to hit both a rename/move link *and* a convert link.

**Take option B:** lead each sentence with the differentiator and state the shared remedy **once**
beneath both.

- "Renaming onto a link destroys it — the link is removed and its target is left orphaned."
- "Creating a file at a link's name writes THROUGH it — the bytes would land at the link's target, a
  path you did not name, and a failure part-way would then delete the link itself."
- then, once: "Nothing was changed; remove the link first if that is what you meant."

That cuts the box by about two lines and puts the distinguishing words where the eye lands.

## Acceptance criteria

- [ ] Restructure the two hazard sentences as above: differentiator first, shared remedy stated once.
- [ ] Keep `genericizeReason()`'s property — **no sentence may name any single collision's path**.
      That was CPE-1891's own fix for a real defect (one path quoted while several were listed) and
      it must survive; the existing test asserting it should still pass unchanged.
- [ ] Handle the single-hazard case gracefully — with only one kind present, the remedy must still
      read naturally rather than as an orphaned line.
- [ ] Re-capture `cpe-1891-{light,dark}-many-blocked.png`, the only evidence that exercises both
      kinds at once.


## Two more doc one-liners folded in here (2026-08-27)

PR #1044's Reviewer raised these as explicit non-blocking follow-ups after approving. They are
one sentence each, and they were **deliberately not pushed to that PR**: it was approved with CI
already running, and a push would have restarted a ~1-hour CI cycle to add two doc sentences.

- [ ] **`macro_run`'s doc comment: the confirmed set is keyed by *name*, not by file identity.**
      The Reviewer probed it — deleting the confirmed occupant and creating a *different* plain file
      at that exact name still authorises the overwrite (`PROBE swapped-file-at-same-name -> BYPASS
      STILL ALLOWED`). Judged acceptable, and I agree: it is a name the user was shown and ticked; a
      link at that name flips `confirmable` false and is refused unconditionally; a hard-linked one
      is refused on the handle; every un-ticked name fails closed. Pinning identity would need a
      handle or dev/ino carried across two IPC calls, which this repo does only *within* a call, and
      Batch-Media's plan-to-execute re-check is likewise path-based across that boundary. Write it
      down so the next reviewer does not rediscover it.
- [ ] **`src/docs/organizing-macros.md`: a Rename template containing a separator now relocates
      within the root.** A real behaviour change on the *unconfirmed* path, and a fix — it used to be
      flattened to the input's own parent while the plan preview, the preflight and the recorded
      inverse all said `sub/`, so undo of such a macro was already broken. It now lands where the
      preview always promised. Pinned by
      `cpe_1891_a_rename_template_with_a_separator_lands_at_the_same_full_to_confirmed_or_not`.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1044's Visual Critic. Purely editorial — nothing
here changes behaviour, and the Critic passed the PR with this noted as non-blocking.

Related: **CPE-1891** (the collision panel), **CPE-1892** (the copy-held-back-paths button's rough
edges), **CPE-1869** (the list-copy pattern both reuse).

## Work Log

**2026-08-27 — implemented (option B, as decided in the ticket; not re-litigated).**

`MacroRunConfirm.svelte` now splits each backend link-refusal `reason` into two parts before rendering:

- **The lead** (`"<path>" is a link, and `) is dropped, so the sentence STARTS on its differentiator —
  `Renaming onto a link destroys it — …` / `Creating a file at a link's name writes THROUGH it — …`.
  Path-stripping is still what CPE-1891 required; it just no longer leaves `This destination is a
  link, and` standing in front of the only words that differ.
- **The tail** (`Nothing was changed/written; remove the link first if that is what you meant`) is
  hoisted out and printed ONCE beneath both sentences, as `Nothing was changed; remove the link first
  if that is what you meant.` ("changed" covers "written").

With a **single** hazard kind present there is nothing to de-duplicate and a lone remedy line would
read as a second, contentless hazard — so it is folded back onto that one sentence and no separate
remedy element renders. A `reason` that does not match the link shape (the guards' "could not check
whether … is a link" arms) falls through to CPE-1891's `genericizeReason` unchanged.

Both-hazards box: **7 lines of near-duplicate prose → 4 lines of two distinguishable sentences + 1
shared remedy line**, differentiators in the first two words of each.

**Tests** — `MacroRunConfirm.test.ts`'s fixtures now carry the **verbatim** backend reasons including
their remedy tails (they had been clipped short of the tail, so nothing could have exercised the
hoist). The wording assertions are pinned as whole-string literals, plus:

- each hazard sentence's assertion is the OTHER's negative case (`RENAME_HAZARD_SENTENCE !==
  CONVERT_HAZARD_SENTENCE`, asserted), so neither can be satisfied by the wrong hazard's sentence;
- the two are asserted to diverge within their **first four words**, not merely somewhere — a test
  that only checked they differ eventually would pass the very shape this ticket is about;
- the remedy is asserted present exactly once (`getAllByTestId("blocked-remedy")` length 1) and absent
  from every hazard sentence;
- the new single-hazard test asserts the folded sentence is specifically the CONVERT one (`contains
  "writes THROUGH it"`, `not.toContain "destroys it"`, `not.toEqual` the rename-flavoured fold), so the
  fold cannot launder the two kinds into one generic sentence.

CPE-1891's no-path property is untouched and still asserted (now against all three fixtures' `to`).

**Also folded in from this ticket's two doc one-liners:**

- `macro_run`'s doc comment now records that `confirmed_overwrite` is keyed by **name, not file
  identity**, with why that is acceptable and what pinning identity would cost. (A doc comment on a
  `#[tauri::command]` ⇒ `src/lib/bindings.gen.ts` regenerated via `export_bindings`, or CI's
  Typed-bindings drift guard reds.)
- `src/docs/organizing-macros.md` now states that a Rename template containing a separator **relocates
  within the root** (`sub/{stem}.{ext}` lands in `sub/`, as the plan preview always promised), and its
  collision bullet is rewritten to describe the new differentiator-first / remedy-once shape (CPE-579).

**Visual evidence** (branch-only, per convention — not committed to `main`), captured off-screen via
headless Edge against the `?case=many` harness, the only case that puts both hazards on screen:

- `.claude/sprint-metrics/visual-evidence/cpe-1928-{light,dark}-many-blocked.png`
- `.claude/sprint-metrics/visual-evidence/cpe-1928-{light,dark}-single-hazard-remedy-folded.png`

Kept under new CPE-1928 names rather than overwriting `cpe-1891-{light,dark}-many-blocked.png`, so the
Critic has the before/after pair side by side and no sibling PR touching that file conflicts.

`npm run check` clean; `npm test` 4693/4693 green (incl. the `bidiEscape.guard` registry, which needed
`text:blockedRemedy` recording — a frontend constant, no backend text in it).

**2026-08-27 — PR #1056 review round: both findings addressed.**

**Finding 1 (blocking), fixed — the split was ordered so one drift direction DELETED the remedy.**
`hazardSentence` stripped the tail and *then* tested the lead, against the already-stripped body. The
stated safety property ("an unrecognised `reason` passes through unchanged") therefore only held when
*neither* half matched. When the tail matched and the lead did not, the remedy was gone from the
sentence — and because `representativeReasons` computes `sawLinkHazard` from the **raw** `reason`, no
separate remedy line rendered either, so "remove the link first" left the dialog entirely. Not
hypothetical: `batch_media.rs` already prefixes this same refusal (`refusing at write time: "<out>"
is a link, and …`). Now the lead is matched against the raw `reason` and the tail is stripped only
once the shape is recognised, which also makes this function agree with the flag
`representativeReasons` was already computing. The opposite drift (tail reworded, lead intact) still
degrades the safe way — the remedy renders twice, never zero times.

New test `keeps the remedy on screen when the reason's LEAD drifts but its tail still matches` uses
the real `batch_media`-shaped prefix and asserts the box still contains `remove the link first`,
that the sentence came through byte-identical, and that the remedy appears **exactly once** (so the
fix cannot be "satisfied" by printing it twice). **Red-proofed:** restoring the old
strip-then-recognise ordering fails it on exactly that assertion — `expected '… 1 destination can't
be overwritten —…' to contain 'remove the link first'`.

**Finding 2, built now (not deferred) — the derivation guard.** Nothing bound the TS wording to the
Rust string: every frontend assertion compared hand-copied constants to hand-copied fixtures, and
every *Rust* assertion on these two messages is a substring check that does not red on a lead reword.
A backend copy edit could pass `cargo test`, pass `npm test`, and silently change the dialog — and,
before Finding 1's fix, silently delete the remedy. A new `describe` block at the foot of
`MacroRunConfirm.test.ts` now reads `crates/server/src/fsutil.rs`, walks the `Ok(true)` arm's
`format!` literal in `classify_symlink_slot` / `classify_create_slot` (resolving `\"`, `\\` and Rust's
`\`-at-end-of-line continuation, which also swallows the next line's indentation), substitutes the
fixture's path for `{}`, and asserts byte-identity with all three fixtures. Two further assertions
pin the halves the splitter depends on: each message still **opens** with `"{}" is a link, and ` and
still **closes** on the shared remedy clause exactly once. Same shape as the repo's existing
source-reading guards (`channelPurityCoverage`, `catalogPublishFreshnessGuard`, `lockfileLockedGuard`).

**Red-proofed twice**, the second deliberately with a reword `cargo test` would *not* catch:

- lead → `is a symlink, and renaming onto a symlink destroys it`: 2 guard tests red (byte-identity +
  the lead-shape check).
- lead → `renaming onto a link **wrecks** it`: the byte-identity guard reds. Confirmed no Rust
  assertion pins that clause — the only `cargo` hits on "destroys it" are doc comments and one
  assert's *failure message*, so this edit is green on the Rust side and red only here. That is the
  guard doing the job it was asked for: failing on the side that caused the drift.

Screenshots are unchanged and still valid — behaviour on every string the backend produces today is
identical through both orderings; only the unrecognised-shape path moved.

`npm run check` clean; `npm test` **4698/4698** green (+5: 1 lead-drift, 4 derivation guard).

**Two pre-existing defects the reviewer found, agreed out of scope here** (Foreman filing them against
CPE-1891): the `could not check whether "…" is a link` arms leak the path into the sentence, since
`genericizeReason`'s `/^"[^"]*"\s*/` is anchored and cannot match a message opening with `could` —
identical before and after this PR; and because dedup is by bucket, a "could not check" rename
collision appearing before a real-link one suppresses the real-link sentence (and now its remedy)
entirely. Both are properties of the bucketing/genericizing that predate this change, and fixing them
means revisiting how a non-link-shaped refusal is bucketed — larger than this XS.
