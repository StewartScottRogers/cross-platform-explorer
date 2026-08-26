---
id: CPE-1875
title: the undefined-token guard enumerates five names instead of detecting the defect, so a sixth occurrence ships green
type: task
priority: Medium
status: Doing
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
- **2026-08-26 USMST** — Picked up by a sprint Worker. Plan: replace the hard-coded `GUARDED_TOKENS`
  coverage block in `src/app.css.warn-token.test.ts` with a real detector that greps every
  `var(--token, <fallback>)` call site across `.svelte`/`.css`/`.ts` and asserts each discovered
  token resolves to a concrete hex in all five live theme blocks. Keep the three existing per-token
  invariants (dead-fallback ban for the original five, literal-hex ban, WCAG contrast pairings)
  unchanged since they carry extra meaning beyond coverage. Expect `--mono` (CPE-1876, ~24 call
  sites) to go red immediately; disposition an explicit dated allowlist pointing at CPE-1876 rather
  than fixing it here.
- **2026-08-26 USMST (attempt 2/3)** — PR #1030 came back CHANGES REQUESTED from an independent
  reviewer, plus a third finding from UAT. All three were real and fixed:
  1. **Component-local exemption was forgeable.** `declaresItself` only checked that `token:`
     appeared anywhere in the file, so `<div style="--newtok: #654321"><span style="color:
     var(--newtok, #654321)"></span></div>` read as "a legitimate local variable" and was invisible
     to every assertion — a masking literal in local-variable costume, functionally identical to the
     CPE-1810/1821/1876 defect. Fixed: the exemption now requires the declared value to be
     genuinely DYNAMIC (contain a `{` — a Svelte mustache or JS template interpolation); a
     static-valued declaration no longer qualifies. `--indent`/`--sw` (both real computed values)
     remain exempt; confirmed via full-suite green (59/59) both before and after the fix, with no
     new failures for either.
  2. **Comment stripper missed `//` line comments.** Only `/* */` and `<!-- -->` were stripped, so a
     future `.ts` doc comment quoting the idiom by name (this file's own 41-line header uses `//`
     throughout) would scan as a real call site and red CI for prose. Fixed with a quote-aware
     `stripLineComments` scanner (tracks quote-string state with escape handling) so a `//` inside a
     string — a `https://` URL, or plain in-string text — survives untouched while a real line
     comment is removed before the token scanner runs.
  3. **Remediation message was dead code.** `expect(undefined).toMatch(HEX_RE)` throws vitest's own
     `TypeError: .toMatch() expects to receive a string, but got undefined` before the custom "add a
     dated ALLOWLIST entry" message is ever used — so a failing developer only ever saw the test
     title, never the advice. Fixed by asserting `toBeDefined()` (with the same message) before the
     `toMatch()` check, in the two places this ticket introduced. The three pre-existing invariants
     below this ticket's rewrite were confirmed byte-identical by the reviewer and were not touched.

  **Red-proof evidence** (all three run against the real repo, confirmed RED/behaving as required,
  then reverted — `git diff --stat` clean before and after each):
  - (1) Added `<div style="--cpe1875-masking-probe: #654321">` + `var(--cpe1875-masking-probe,
    #654321)` to `FileList.svelte` -> 5 failures (one per theme block), e.g.:
    `--cpe1875-masking-probe (referenced from src/lib/components/FileList.svelte) did not resolve to
    a hex in bare :root (default) — got raw value undefined. If this is known, pre-existing debt,
    add a dated entry to ALLOWLIST above pointing at the ticket that owns it — do not fix the
    underlying token here unless that IS the ticket this work is filed under.: expected undefined to
    be defined` — proves both the masking-literal catch AND (see below) the remediation message
    reaching output. Confirmed `--indent`/`--sw` (genuinely dynamic) remained exempt throughout
    (59/59 still green after revert).
  - (2) Added to `src/lib/colorRules.ts`: a real `//` comment quoting `var(--cpe1875-linecomment-
    probe, #654321)` by name (must NOT be flagged), plus `"https://example.com/docs" + "
    var(--cpe1875-urlline-probe, #654321)"` and `"not a url // just text" + "
    var(--cpe1875-stringslash-probe, #654321)"` (both real call sites, each preceded by a `//`
    inside a string, must survive). Result: `--cpe1875-urlline-probe` and
    `--cpe1875-stringslash-probe` both failed correctly (5 failures each, all theme blocks) —
    proving the URL and in-string-slash cases survived the stripper. A temporary diagnostic dump of
    `discovered.keys()` confirmed `--cpe1875-linecomment-probe` was ABSENT from the discovered set:
    `CPE1875_DISCOVERED_KEYS: ["--accent","--cpe1875-urlline-probe","--cpe1875-stringslash-probe",
    "--surface","--text","--border","--surface-alt","--border-strong","--mono","--selection",
    "--text-dim","--bg"]` — the real comment was correctly stripped and never scanned.
  - (3) Demonstrated by probe (1)'s own output above: the full custom message ("...did not resolve
    to a hex... If this is known, pre-existing debt, add a dated entry to ALLOWLIST above...")
    appears verbatim in the `AssertionError`, not swallowed by vitest's `.toMatch()` TypeError.

  Guardrails re-run clean after the fix: `npm run check` — 0 errors, 0 warnings. `npx vitest run` —
  331 files / 4479 tests, all green. Rebased onto `main` (moved: CPE-1734, CPE-1869, CPE-1889) and
  pushed attempt 2.
