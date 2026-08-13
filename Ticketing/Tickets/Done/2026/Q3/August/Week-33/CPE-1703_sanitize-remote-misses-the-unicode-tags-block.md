---
id: CPE-1703
title: sanitize_remote misses the Unicode Tags block, the ASCII-smuggling vector for hidden agent instructions
type: bug
priority: Medium
status: Done
tags: ready
estimate: XS
created: 2026-08-13
closed: 2026-08-13
---

## Problem

Found by the independent reviewer on PR #884 (CPE-1700), which probed `is_format_control` character by
character rather than trusting its range list.

CPE-1700 added `is_format_control()` to neutralise Cf-class characters in remote-supplied S3 error text —
closing the Trojan Source bidi-override hole. It covers `U+00AD`, `U+061C`, `U+200B..=200F`,
`U+202A..=202E`, `U+2060..=2064`, `U+2066..=2069`, `U+FEFF`: every character the ticket named, plus the
adjacent bidi and zero-width siblings.

It does **not** cover the full Unicode Cf (Format) category. The notable omission:

- **`U+E0001` and `U+E0020`–`U+E007F` — the Tags block.** Genuine Cf codepoints, invisible in normal
  rendering, and the exact mechanism behind the documented 2023–2024 "ASCII smuggling" / hidden-instruction
  attacks, which hide a payload inside otherwise-normal-looking text for a downstream LLM to read.

Lower relevance, same category: `U+206A`–`U+206F` (deprecated format characters) and scattered
Arabic/Syriac format marks (`U+0600`–`U+0605`, `U+06DD`, `U+070F`, `U+08E2`).

## Why the Tags block specifically matters in this repo

An S3 error `message` is remote-controlled text from whatever endpoint the user pointed at. This codebase
has heavy AI-agent tooling — Agent Watch, the AI Console sidecar — so error text is plausible content to
eventually reach an agent's context window. Hidden instructions smuggled through the Tags block are
invisible to the human reading the error and legible to a model reading the same string.

That is a different threat from Trojan Source (which deceives a *human* reader); this one deceives a
*model* reader. CPE-1700 closed the first and left the second.

## Not urgent, and not a regression

Stated plainly so this is prioritised honestly:

- The authoritative `code` is ASCII-only after CPE-1700's shape check, so nothing smuggled can affect the
  verdict a user or a log-scraper reads.
- Before CPE-1700, **zero** Cf characters were filtered. This is an incomplete improvement, not a
  new hole.
- No path today carries an S3 error message into an agent context. The concern is that one plausibly could.

The one thing that is wrong today is the claim: PR #884's title and module doc say it "closes the Cf-class
gap", which overclaims — it closes the *named* gap. (The function's own doc comment is more careful and
scopes itself to the named ranges.) Fix the wording along with the ranges.

## Scope

`crates/s3/src/error.rs` — `is_format_control`.

## Acceptance criteria

- [ ] `U+E0001` and `U+E0020`–`U+E007F` are neutralised, with a test naming ASCII smuggling / hidden
      instructions as the reason so nobody later deletes it as paranoia.
- [ ] Decide on `U+206A`–`U+206F` and the Arabic/Syriac format marks — include or exclude, with the
      reasoning recorded. Excluding is acceptable; silently omitting is not.
- [ ] Decide whether to cover Cf **by category** rather than by hand-listed ranges, and record the
      trade-off. A hand-rolled list was chosen deliberately to avoid a `unicode-*` dependency (lean-core
      guardrail), which is a legitimate reason to keep hand-listing — but the cost is exactly this ticket
      recurring the next time someone finds an uncovered range. Say which cost you are choosing.
- [ ] **The legitimate-text regression bar must not shrink.** The reviewer verified CJK, accented Latin,
      emoji, Arabic content, Hebrew content, Cyrillic and Greek all survive untouched, and that
      `"中文 café ü Ω д ا ب א ב 😀🔥 \u{202E}RLO\u{202D}LRO"` passes through with only the two bidi overrides
      replaced. Re-run that and keep it.
- [ ] Correct the "closes the Cf-class gap" wording to match what is actually covered.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per
      the Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #884 review, 2026-08-13. The reviewer explicitly declined to block on it —
not exploitable within the ticket's threat model, and strictly better than the pre-PR state.

Related: **CPE-1700** (which added the function), **CPE-1682** (the error path it sanitises), and the
Agent Watch / AI Console surfaces that make model-readable text a live consideration.
