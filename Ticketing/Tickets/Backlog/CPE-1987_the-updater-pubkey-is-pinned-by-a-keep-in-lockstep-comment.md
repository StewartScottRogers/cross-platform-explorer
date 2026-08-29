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
