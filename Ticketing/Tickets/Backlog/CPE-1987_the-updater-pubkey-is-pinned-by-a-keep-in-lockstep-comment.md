---
id: CPE-1987
title: the updater pubkey and endpoint are pinned by a **"keep these in lockstep"** comment — a bare provenance claim on the root of trust
type: bug
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-28
---

## Summary

Found by **PR #1105**'s Security Auditor (CPE-1900) while clearing that PR, and correctly kept out of it —
it is **pre-existing and unchanged by that work**, and folding a root-of-trust change into a PR about
config-chain derivation would have been the scope creep this repo keeps refusing.

`src/lib/sidecarBundleResources.test.ts:469` carries:

> *"Keep these two literals in lockstep with … `pinned_pubkey.rs`"*

**That is a bare provenance claim, on the root of trust, with nothing checking it.** CPE-1933's exact
shape: a comment asserting that code here reproduces something *there*, untested by construction — and
**worse than no comment, because the surrounding green test reads as vouching for it.**

The values are the updater's **pubkey and endpoint**. A drift between the test's literals and the Rust
const does not break a build; it silently means **the pin is no longer pinning what ships.**

## Why it is cheap, which is the reason it is a ticket and not a note

**The file already does exactly this, for a different value, three hundred lines away.** It imports
`rustStrSliceAfter` and uses it to derive `TAURI_PLATFORM_TOKENS` **out of the same crate at run time**.
Deriving the pubkey the same way is a few lines, and the machinery is already sanctioned:
`src/lib/rustSource.ts` (`stripRustComments`, `rustStringLiteralAfter`, `rustStrSliceAfter`) exists
precisely so a second private scanner does not grow.

**The Auditor's independent measurement, worth keeping:** the pinned pubkey and endpoint are currently
**byte-identical across all four sites** — the test literal, its pre-PR value, the Rust const, and
`tauri.conf.json`. So this is a live-and-correct value with a dead check, not a drift that has already
happened.

## What this needs

- [ ] **Derive the pubkey and endpoint from `pinned_pubkey.rs` at run time**, using `rustSource.ts`'s
      existing helpers — **not** a new scanner, and **comments stripped first**. A raw-text scan over Rust
      that matches a value quoted **in a comment** is the silent-pass shape this repo has now measured
      three separate times.
- [ ] **Red-proof it in both directions** and write the counts at the site: change the Rust const → red;
      change the test literal → red. **A derivation that never actually re-reads its source is the same
      defect with extra steps.**
- [ ] **Enumerate the sites rather than recalling them** (CPE-1932). The Auditor found **four** carrying
      these values (test literal, Rust const, `tauri.conf.json`, plus the pre-PR value in history) —
      derive the live list rather than trusting that number, and **report a verdict per site including the
      ones that are fine.**
- [ ] **Do not weaken CPE-1873's pins while restructuring.** The updater assertions over the merged config
      must still red on an attacker key injected into **any** chain file, on **all** shipped legs — after
      CPE-1900 that is **six** derived legs, not three. Re-run that injection and report it.
- [ ] **Never commit a signing key**, and **do not touch `tauri.conf.json`'s real `pubkey`/`endpoints`**
      except as a reverted injection inside a red-proof. Verify with `git status --porcelain` before
      finishing.

## Notes

Filed 2026-08-28 by the sprint Foreman from PR #1105's Security Auditor, which measured it as pre-existing
and flagged it as *"worth its own ticket rather than a demand here."*

Related: **CPE-1900** (PR #1105 — the config-chain derivation that made this visible, and the worked
example of deriving instead of listing), **CPE-1873** (the pin itself), **CPE-1933** (derive provenance,
don't claim it — and anchor on code, never on prose), **CPE-1950** (a shared oracle catches divergence,
not shared blindness; where duplication is removable, remove it), **CPE-1932** (enumerate, don't recall).

## STATE AT SPRINT WRAP — 2026-08-29 01:10. PR #1108 is OPEN and MUST NOT be merged as it stands.

Three review rounds, **three working bypasses**, each a *different* way for the scanner's notion of "the
declaration" to diverge from **rustc's**. Every one was demonstrated end to end at **the same three files**
— attacker pubkey in an overlay, that overlay added to `release.yml`'s three matrix `args:`, and one edit
in `pinned_pubkey.rs` — with the **whole suite green, clippy clean and the Rust tests 8/8 ok.**

| round | the rule | the bypass |
|---|---|---|
| 1 | first textual occurrence of the anchor | a decoy declared **above** the real one (`_LEGACY` name, a `#[cfg]` duplicate, or the anchor inside a raw string) |
| 2 | refuse a **second** occurrence | make the real declaration not match — **one extra space after `pub`** — and plant the anchor **once** in a raw string or `#[doc]`. Count stays at 1. |
| 3 | match a **declaration** (line start → anchor, spaces widened → `:`) and count those | make the real declaration not *declaration-shaped* — **`#[rustfmt::skip] pub const K: …` on one line** — and plant one that is. Count stays at 1. |

**Round 3's remaining hole is the one to read first.** Its docblock declares three blind spots — macro-generated,
same-line, and `pub\nconst NAME` — and says **"all report not found — loud, and the safe direction."** That was
measured on a file containing **only** the real declaration. **The attacker also controls whether a second
declaration-shaped line exists.** Measured with a decoy present, all three — plus form-feed and vertical-tab
indents (Rust whitespace, not on the list) and a second declaration on one line — **silently derive the decoy.**

`#[rustfmt::skip]` on a 200-character base64 line is the least suspicious edit of the three rounds. **It reads
as housekeeping, and it also neutralises CPE-1991 before that ticket lands.**

### The fix to take, and why it is different in kind

Do **not** enumerate a fourth shape. Add a second, deliberately **looser** scan whose only job is to disagree:

```ts
const strict = [...src.matchAll(declPattern(anchor))];   // the declaration we will read from
const loose  = [...src.matchAll(new RegExp(escape(name), "g"))];  // the name, anywhere at all
if (loose.length !== strict.length)
  throw new Error(`… occurrences that are not left-margin declarations …`);
```

**In an honest file both counts are 1.** Any divergence — a decoy, *or* the real declaration in a form the
matcher cannot see — is refused, **because from inside the reader those two are indistinguishable.** That
converts every row above into a loud refusal and **stops the sequence rather than extending it**; round 3's
D/D2 shapes become refusals again, which is the conservative direction for a root-of-trust pin.

**Three claims must be reworded in the same commit**, because each is now measurably false:
`rustSource.ts:307-311` ("all report not found — loud, and the safe direction"); the `rustSource.test.ts` leg
whose comment extends its fixture's result to "the same class as a macro-generated const"; and
`pinned_pubkey.rs:147-148` rotation step 4 ("anything else reds at collection rather than silently").

### Also outstanding

- **CLAIM-5:** the always-refuse sabotage figure at `rustSource.ts:319-320` records **21F/21P**; measured
  **20F/22P**. Second time a sabotage figure at this site has not reproduced. The regression sabotage does
  reproduce exactly (**11F/31P**, 4 of them the variant D/D2 legs).
- The twin leg at `rustSource.test.ts:235` carries none of the caveat its sibling now has.
- *"The check that it was read off the live declaration is `uniqueAnchorIndex`"* is **the property SEC-3
  breaks** — it becomes true only once SEC-3 is fixed.

### What IS closed and verified, so round 4 does not re-derive it

SEC-1 and SEC-2 both genuinely closed (D and D2 re-run against the **real** `pinned_pubkey.rs`, 90/90, and
green there means *read the live key*). The third verdict flip is correct and correctly reasoned, with new
block-comment pins firing in both directions. Two refusals correctly became reads (`_LEGACY`, mid-line raw
string); the `#[cfg]` refusal and the column-0-in-raw-string refusal are unchanged. Four ordinary spellings
still read. CLAIM-4 is properly placed in rotation step 4 itself. The CPE-1873 sweep is unchanged
(**12/36** widest, **2/46** narrowest). Gates at the clean head: **364 files / 5,560 passed / 62 skipped**,
`npm run check` 0/0, clippy clean, `cargo test -p cpe-updater-verify` 8/8, `cargo doc` clean — Windows only,
no Rust code added, `src-tauri/` absent from the whole PR diff, no key material committed.

**Related, filed from this work:** **CPE-1991** (there is no `cargo fmt --check` anywhere, which is what made
round 2's whitespace half invisible — **defence in depth, explicitly not the fix**).
