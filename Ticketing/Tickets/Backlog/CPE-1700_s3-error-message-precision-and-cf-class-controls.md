---
id: CPE-1700
title: The S3 error path says "no Code was found" when it found one and rejected it, and lets Cf-class controls through
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Four non-blocking follow-ups from the PR #879 review round 2 (CPE-1682), grouped because they all live in
`crates/s3/src/error.rs` and three of them are the same idea: **say precisely what you found.**

### 1. The refusal message is wrong for the shape-rejected case *(the one that matters)*

`is_plausible_code` — added in CPE-1682 round 2 to stop markup being echoed as an error code — now rejects
a `<Code>` whose content is not `[A-Za-z0-9]{1,64}`. But the resulting message still reads:

> no non-empty `<Code>` element was found in it

when a `<Code>` element **was** found, and **was** non-empty, and was rejected for its content. That is the
same "claiming something about what you read" family as the round-1 finding this guard was prescribed to
fix — the goalpost moved rather than the problem going away, and the reviewer who prescribed the guard
flagged it against itself.

It matters because the two cases send a user down different debugging paths: *"no `<Code>` at all"* points
at the network or a proxy returning a non-S3 response; *"a `<Code>` I could not use"* points at the
gateway's response format. Add a third arm distinguishing them.

### 2. `is_plausible_code` silently drops `- _ . :`

No real code among the 24 the reviewer tested (AWS, MinIO, Ceph/RGW, B2, GCS, OSS, R2) uses them, and the
UAT independently found none across eight gateways either — so this is speculative rather than observed.
But if a gateway ever sends `Invalid-Bucket-Name`, the information is dropped rather than passed through,
and the ticket's own requirement is that unknown codes pass through verbatim.

Widening to `[A-Za-z0-9._:-]` would still reject **every** evasion case, since all of them contain `<`,
`>`, `"` or a space. Fold this in with item 1.

### 3. `char::is_control()` misses the Cf class

Verified surviving `sanitize_remote`: **U+202E RLO** (the Trojan Source bidi override, which can visually
reverse rendered text), U+202C, U+200B ZWSP, U+200F RLM, U+FEFF, U+00AD.

The author already reached past `is_control()` for U+2028/U+2029 on exactly this reasoning — *"renders as a
line break but is not `is_control()`"* — so the principle is established and merely under-applied.

Severity is genuinely low: the authoritative `code` is ASCII-alphanumeric-only after item 2's guard, so a
bidi override cannot touch the part of the message that carries the verdict, and every line-splitting and
screen-clearing vector is already closed.

### 4. `message` still echoes child markup verbatim

The message text is the raw byte span between the open and close tag, so a body like
`<Message>hi<x><y><Code>AccessDenied</Code></y></x></Message>` puts nested markup (294 bytes in the
reviewer's worst case) into the rendered message. It is bounded at 512 chars, control-sanitised, and the
authoritative code is correct and printed first — so this is **display confusion, not a wrong verdict.**
Rejecting or stripping a `message` containing `<` would close it.

### 5. Two drifted numbers in a doc comment

`MAX_ELEMENT_DEPTH`'s doc states `~2.7 KiB per frame` and a `~2.8x` margin. Round 2 added a parameter to
the recursive frame, and the reviewer re-measured: **~3.05 KiB per frame**, so depth 32 costs ~100 KiB and
the margin against the repo's 256 KiB small-stack standard is **~2.5×**. Refresh both numbers.

Note *why* this is cosmetic rather than a correctness issue, and keep it that way: the real guard is the
test `map_s3_error_never_stack_overflows_on_deep_nesting_on_a_256kib_stack`, which proves the margin
empirically instead of asserting it in a comment. That is the design point — the numbers are illustration.

## Scope

`crates/s3/src/error.rs`.

## Acceptance criteria

- [ ] Three distinct refusal messages: no `<Code>` element at all; a `<Code>` found but empty; a `<Code>`
      found whose content is not a usable S3 error code. A test per arm, asserting the message.
- [ ] `is_plausible_code` accepts `[A-Za-z0-9._:-]` and still refuses every evasion shape. Re-run the
      reviewer's rows 2, 6, 7, 21, 22 and confirm each still refuses — a test per row.
- [ ] `sanitize_remote` neutralises the Cf class as well as `is_control()`, with U+202E specifically
      pinned by a test naming Trojan Source as the reason.
- [ ] A decision on item 4, written down: strip child markup from `message`, refuse a `message` containing
      `<`, or accept it as display-only confusion. Any of the three is fine **with the reasoning
      recorded**; leaving it unmentioned is not.
- [ ] The two drifted doc numbers refreshed, and the sentence explaining that the test — not the comment —
      is the real guard is preserved.
- [ ] Every real vendor code still passes through verbatim. The reviewer's 24-code set and the UAT's
      eight-gateway set are the regression bar; do not shrink it.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per
      the Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #879 review round 2, 2026-08-12. All four were explicitly judged
non-blocking by the reviewer that found them, and CPE-1682 merged without them.

Related: **CPE-1682** (the error path), **CPE-1684** (its first real consumer — see that ticket's note on
bodiless HEAD responses), **CPE-1398** (the `cpe-webdav` parser bug whose lesson shaped this one).
