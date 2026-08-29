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

## Work Log

**2026-08-28 — built.** The two literals in `sidecarBundleResources.test.ts` are gone; the file reads
`EXPECTED_TAURI_UPDATER_PUBKEY` / `EXPECTED_TAURI_UPDATER_ENDPOINTS` out of `pinned_pubkey.rs` at run
time, comments stripped first. New helper `rustStrConstAfter` in `src/lib/rustSource.ts` (the scalar
sibling of the existing `rustStrSliceAfter`) — not a new scanner; the reciprocal claims in
`pinned_pubkey.rs`'s module doc, its rotation procedure step 4, and `README.md`'s "pinned in THREE
places" were updated in the same commit.

**The framing the ticket inherited was slightly off, and the correction is the interesting part.**
A stale TS literal could not have drifted *silently* — it was compared against the real merged config,
so a stale copy simply reds. What the copy actually bought was the reverse: because it was independent
of the Rust const, an attacker writing a key into an **overlay** *and* into that literal hid it from the
only guard that could see it (the Rust pin only ever reads the BASE config, untouched in that
scenario). Deriving closes that. The cost, stated at the site: the deleted third copy also used to red
on a `tauri.conf.json` + `pinned_pubkey.rs` rotation, and no longer does — which is the same
self-consistency limit `pinned_pubkey.rs`'s "What none of this proves" already declares out of bounds.

**Corrected after review (PR #1108, CLAIM-1) — the first write-up of that attack did not reproduce.**
It said "two files, six shipped legs, nothing red". Measured on base `dd560869`, the **two**-file
version (overlay + the literal) is **3 failed / 44 passed**: `release.yml`'s plain channel takes no
overlay, so its three legs keep the real merged pubkey and say so. Only the three **sidecar** legs are
compromised, and it is **not silent**. The genuinely all-green shape needs a **third** file — the
overlay added to `release.yml`'s matrix `args:` — and that one does reproduce in full (whole suite
green, attacker root of trust on all six legs). The fix holds either way: at head, the two remaining
files of that attack red **6 failed / 42 passed**, every leg.

**Site enumeration** (`git ls-files | xargs grep -l`, not recall). Three live files carry the pubkey —
`pinned_pubkey.rs` (source of truth, unchanged), `src-tauri/tauri.conf.json` (unchanged), and the test
(literal deleted, now derived). Two more carry the endpoint URL in prose only: `RELEASING.md` and a
Done ticket — both fine, neither is a pin. The Auditor's "four" counted a history value, which is not
a live site.

**Red-proofs (all reverted; `git status --porcelain` clean after each).** Rust pubkey const changed →
6 failed / 42 passed. Rust endpoints const changed → 6 failed / 42 passed. Rust const renamed → the
reader throws at collection, whole file reports zero tests. Rust const blanked → 8 failed / 66 passed
across the two files, the vacuous-derivation leg naming itself first. Decoy `pub const
EXPECTED_TAURI_UPDATER_PUBKEY` planted in a `//` comment → 48/48 green with stripping, while an
unstripped reader derives the decoy (measured with a throwaway script).

**CPE-1873's injection re-run over all 7 chain files**, unchanged from CPE-1900's recorded table:
12 / 6 / 4 / 2 / 2 / 2 / 2 failures, each reddening exactly the OSes that file governs.

**2026-08-29 — review round 2 (PR #1108: SEC FINDINGS + CHANGES REQUESTED). SEC-1, HIGH, fixed.**
The derivation re-opened the same green-when-compromised shape one file over: both readers took the
**first textual occurrence** of the anchor, where rustc resolves the name by compiler rules. Three
shapes split them and all three derived a decoy silently — a longer name with the anchor as its prefix
(`…_PUBKEY_LEGACY`), a `#[cfg(target_os = "android")]`-gated duplicate (never compiles, invisible to a
text scan), and the anchor inside an earlier `r#"…"#` raw string (which `stripRustComments` correctly
preserves, because a raw string is code). The reviewer built it end to end at the first head:
**74/74 passed**, clippy clean, `cargo test -p cpe-updater-verify` 8/8 ok, attacker root of trust on
all six legs. The Rust legs were immune throughout; only the TypeScript leg — the one that covers
overlays — was fooled.

Fixed with one shared `uniqueAnchorIndex` used by **both** `rustStrConstAfter` and the pre-existing
`rustStrSliceAfter` (`EXPECTED_TAURI_UPDATER_ENDPOINTS` had the identical hole): the anchor must occur
exactly once, or the reader refuses rather than guessing. All three shapes leave it occurring twice, so
one check closes the class. Re-measured by planting each decoy in the real `pinned_pubkey.rs`: each now
throws at collection, naming the second occurrence's line. **This revises the earlier scope call** —
"no guard against a fourth copy of the value" is no longer the shape of the risk; the derivation made a
*second declaration of the anchor* load-bearing, and that is guarded in the reader by one added line
rather than by editing every pin. Recorded at the site: the TS pin now trusts this file's text where
Rust trusts the compiler, **strictly weaker than the independent literal it replaced was independent** —
still net positive, but only with the uniqueness check.

Two pre-existing legs changed verdict as a result, in the safe direction: the raw-source half of each
comment-decoy test now **throws** ("anchor is not unique") instead of silently returning the stale
value. Stripping's value is unchanged; forgetting it is no longer silent.

**CLAIM-2 (low), corrected.** The CPE-1929 write-up quoted the wrong spelling: `semi >= -1` alone is
**26/26 green** because `&& quote > semi` still gates it; the always-refuse spelling is
`semi >= -1 || quote > semi`, and that reds 4. "Shadowed" is also a property of the **order**, now
stated at the site: re-inserted *after* the whitespace check it is 26/26 both ways; *before* it, 1 red.

**CLAIM-3 (low), fixed.** `README.md` still said "Update all three constants" and "all three pins
together" ~15 lines below the opening this PR had corrected — the deleted THREE-places framing, stale
on arrival, in the file being edited. Both rewritten. `RELEASING.md`'s endpoint quote now says at the
site that it is illustrative and not a pin. `rustStringLiteralAfter`'s undecoded numeric escapes
(`\u{2014}`, `\x41`) added to its gap list as a silent-wrong-value gap, unreachable for the
base64/ASCII values read through it today.

**2026-08-29 — review round 3 (`SEC FINDINGS` again). SEC-2, HIGH, fixed: the same attack, the same
three files.** Round 2's rule counted *occurrences* of the anchor; the attack only ever needed the
first one. It never asked whether the **one** occurrence it accepted was a **declaration**. So: spell
the real declaration so it does not match the anchor — **one extra space after `pub` is enough**, and
there is **no `cargo fmt --check` anywhere in `ci.yml`** to normalise it — then plant the anchor
**once**, in a raw string (variant D) or a `#[doc]` attribute (D2), both of which are *code* and
survive `stripRustComments` by design. Occurrences: **1**. The rule accepted it and derived the
attacker value: reader poisoned **6 failed / 77 passed**; the full three-file attack **83/83 passed**,
whole suite green, clippy clean, `cargo test -p cpe-updater-verify` 8/8 ok, attacker root of trust on
all six shipped legs. The round-2 leg did not catch it either — `startsWith(minisign preamble)` and
`length > 80` are satisfied by any attacker minisign key, and that is now said at that leg.

`uniqueAnchorIndex` now matches a **declaration** and counts those: line start → optional indent →
the anchor with every run of spaces widened to `[ \t]+` → optional space → the `:` of the type
annotation. Widening the spaces is what defeats the `pub  const` half inside the reader, so the fix
does not depend on a formatter job landing. Measured against the real `pinned_pubkey.rs` with each
decoy planted: D and D2 each leave **one** occurrence, where the round-2 rule derives
`…IEFUVEFDS0VSCg==` ("ATTACKER") and the new rule derives the live key; a decoy written as a
line-start declaration *inside* a raw string reds instead, naming both declaration lines (149, 151).

Side effects, all in the safe direction and all pinned: the `…_LEGACY` and mid-line raw-string shapes
are no longer refusals but **correct reads** (`pub const K_LEGACY` is not `pub const K` + `:`), the
`#[cfg]`-duplicate refusal is unchanged, and a **third** pre-existing leg changed verdict — the
raw-source half of each comment-decoy test now reads the real value rather than throwing, because a
`// Was: …` line is not a declaration. That is a property of *that comment shape*, not of comments, so
both tests now also pin a column-0 decoy inside a **block** comment, which still needs stripping and
reds without it.

**Both directions sabotage-measured on the new rule (42 tests in `rustSource.test.ts`), reverted, and
the numbers are at the site.** Always-refuse → **21 failed / 21 passed**, complement legs included, so
an over-eager rewrite cannot pass. Regress to round 2's substring counting → **11 failed / 31 passed**,
with the four D/D2 legs among them. Blind spots stated as "at least these" and split by direction: a
macro-generated declaration, one after something else on the same line, and `pub\nconst NAME` all
report **not found** — loud, and not read.

**CLAIM-4 (low), fixed — and the reviewer's own correction taken.** The "do not add a second
declaration" bullet was *not* next to rotation step 4; step 4 sat ~70 lines below and still read
"Nothing to do for `sidecarBundleResources.test.ts` … it just no longer needs editing", which is the
sentence a rotator actually follows and the one place still saying "that file is not your problem".
Step 4 now says there is no **value** to edit *and* that step 3 must leave each const as exactly one
ordinary left-margin declaration, with the failure mode named.

**Two smaller round-3 corrections.** The leg titled "names the line of the second occurrence" over-
promised — every shape measured plants the decoy first, so the second line named is the *real*
declaration; it now names both and is titled accordingly. And the no-strip sabotage leaving
`sidecarBundleResources.test.ts` green is recorded as what it is: on today's file the strip changes no
derived value, so stripping's protection there is **prospective and fixture-covered**, not a live
measurement.

CPE-1873 injection sweep re-run after this fix: **12 / 6 / 4 / 2 / 2 / 2 / 2**, unchanged. Full suite
364 files / **5560 passed** / 62 skipped. The `cargo fmt --check` gap is the reviewer's separate
ticket and was deliberately not widened into here.
