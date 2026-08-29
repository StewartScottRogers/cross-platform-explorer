---
id: CPE-1991
title: there is no `cargo fmt --check` job anywhere, so a whitespace-only edit to Rust is invisible — and it is half of a demonstrated attack
type: task
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-29
---

## Summary

Found by **PR #1108**'s Security Auditor (CPE-1987) while building a bypass of that PR's new pin reader.
**`.github/workflows/ci.yml` contains no `cargo fmt --check` step at all** — not for `crates/server`, not
for `src-tauri`, not for any of the sidecar crates.

On its own that is a style gap. **What makes it a ticket is that it was one half of a working attack.**

The reader that derives the updater pubkey out of `crates/updater-verify/src/pinned_pubkey.rs` matched an
anchor string. The bypass was: **add one extra space to the real declaration so it no longer matches**, and
plant the anchor once somewhere the reader would find first. Anchor occurrences stay at **one**, the
uniqueness check accepts it, and the derived pin becomes the attacker's. Measured end to end — three files
edited, **attacker root of trust on all six shipped legs, whole suite green, clippy clean, Rust tests 8/8
ok.**

**`pub  const` with two spaces is not a thing a reviewer's eye stops on**, and nothing in CI would have
said a word.

## Scope, and what this is NOT

**This is not the fix for that attack.** PR #1108 closes it properly by matching the declaration
structurally (line-anchored, whitespace-tolerant, followed by the type colon) rather than counting
substring occurrences. **A formatter check is defence in depth, not the guard** — do not file it as though
it were, and do not let its presence become an argument for a weaker matcher later.

What it buys is that **the whitespace half of that shape becomes visible in a diff and red in CI**, which is
worth having on a repo where a scanner reads Rust source as text in three places
(`src/lib/rustSource.ts`'s three helpers, `crates/updater-verify`'s workflow scan, and
`MacroRunConfirm.test.ts`).

## What this needs

- [ ] **Enumerate the Rust manifests at run time** (CPE-1932) rather than listing the crates someone
      remembers — this repo has **17** `Cargo.lock` files and a history of a hard-coded list going stale.
      Fail loudly if the enumeration comes back near-empty.
- [ ] **Decide `--check` vs `--check --all` vs per-crate**, and say what it costs. `cargo fmt` has its own
      opinions; a repo that has never run it will almost certainly red on existing files. **Measure how many
      files it wants to change before deciding whether to reformat the tree, add a `rustfmt.toml` that
      matches current style, or gate only newly-touched files.** Do not reformat 17 crates as a side effect
      of a CI ticket.
- [ ] **Cap the job with a measured `timeout-minutes`** per CPE-1967 — a round number is what that ticket
      exists to stop. Derive it from a real run and record the sample beside the value.
- [ ] **Red-proof it**: add a whitespace-only change to a Rust file and confirm the job reds naming the
      file. Then confirm a legitimately-formatted change stays green. **Write both results at the site.**
- [ ] **Say at the site what it does not cover.** It normalises whitespace; it does **not** make a Rust
      source safe to read as text. The three text-readers above still need their own structural matching —
      name them so the next person does not treat `fmt` as having closed that class.

## Notes

Filed 2026-08-29 by the sprint Foreman from PR #1108's Security Auditor, which found the gap while
demonstrating that one extra space defeats a substring-anchored reader.

Related: **CPE-1987** (PR #1108 — the pin derivation, the `uniqueAnchorIndex` rule and the SEC-2 bypass
this is half of), **CPE-1967** (measured job timeouts), **CPE-1932** (enumerate, don't recall),
**CPE-1933** (anchor on code, never on prose — the rule this attack exploited the letter of).
