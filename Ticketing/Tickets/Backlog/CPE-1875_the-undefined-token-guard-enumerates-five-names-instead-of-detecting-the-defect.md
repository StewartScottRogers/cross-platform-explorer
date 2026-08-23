---
id: CPE-1875
title: the undefined-token guard enumerates five names instead of detecting the defect, so a sixth occurrence ships green
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-23
closed:
---

## Problem

`src/app.css.warn-token.test.ts` guards a **fixed list** of five token names —
`--warn`, `--warn-fill`, `--text-muted`, `--accent-2`, `--bg-dim`. Anything outside that list is
invisible to it.

Demonstrated by the independent Reviewer on PR #1009 (CPE-1821), who added a brand-new,
never-defined token at a fresh call site — `var(--text-secondary-2, #123456)` in `FileList.svelte`,
the exact shape of the next occurrence — and ran the suite:

> `app.css.warn-token.test.ts` passed all 37 assertions unchanged.

The only thing that caught the probe at all was `app.css.test.ts`'s hard-coded-hex ratchet (400
against a 399 baseline) — a blunt, coincidental catch that any future author defeats by bumping the
baseline number, which is precisely the failure mode this defect class keeps producing.

## The history that makes this worth closing

This is the same bug three times:

- **CPE-1810** fixed `--warn` and added a named guard for it.
- **CPE-1821** found the identical shape at `--text-muted`, `--accent-2` and `--bg-dim` — three more
  tokens, 22 call sites, the fallback hex applying in **every** theme including both high-contrast
  ones — and extended the named guard by three names.
- Nothing yet stops a fourth.

Each round costs a full ticket to find by hand what a generic check would surface in CI the moment it
lands. Enumerating the known instances of a defect never catches the next one; that is the point of
the guard.

## What to do

Replace the fixed list with a **detector**:

1. Grep every `.svelte` (and any `.css`/`.ts` that emits custom properties) for the fallback idiom
   `var(--some-token, <fallback>)`.
2. For each token found, assert it resolves in **all five** theme blocks — bare `:root`,
   `[data-theme="light"]`, `"dark"`, `"hc-light"`, `"hc-dark"`.
3. Fail with the token name, the file it was referenced from, and which blocks are missing it.

Keep the existing named assertions if they carry extra meaning (contrast pairings, specific values);
this ticket replaces the *coverage* mechanism, not the per-token checks.

**Prove it catches the next one:** the acceptance test is the Reviewer's own probe — add a fresh
undefined token at a new call site and watch the suite go red. If it stays green, this ticket is not
done.

Consider whether the fallback idiom should be **banned outright** where a token exists, since a
fallback that never applies is dead code and a fallback that does apply is this bug. Decide, and log
the reasoning.

## Acceptance criteria

- [ ] A newly-added `var(--undefined-token, #hex)` at any call site fails CI.
- [ ] The failure names the token, the referencing file, and the missing theme blocks.
- [ ] The five currently-guarded tokens remain guarded.
- [ ] The `app.css.test.ts` hex ratchet is no longer the only thing standing between this defect and
      a green build.

## Work Log

- **2026-08-23 14:30 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  on the independent Reviewer's recommendation from PR #1009. The reviewer marked it explicitly as a
  follow-up rather than a blocker, since CPE-1821's own acceptance criteria asked to extend the named
  guard, not to build a detector — so #1009 merges and this ticket carries the general fix.
