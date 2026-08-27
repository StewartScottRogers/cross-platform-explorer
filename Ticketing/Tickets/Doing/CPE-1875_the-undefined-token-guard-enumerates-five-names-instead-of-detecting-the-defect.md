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
- **2026-08-26 USMST (attempt 3/3, hard cap)** — PR #1030 came back CHANGES REQUESTED a second time.
  Foreman specified the exact replacements rather than leaving the shape to another guess, since two
  successive heuristics for "is this local-variable exemption legitimate" had each been defeated by
  a new forgery of the same shape:
  1. **`declaresItself`'s "contains `{`" check was defeated.** `style="--newtok: {'#654321'}"` (a
     braced STRING LITERAL) and `style="--newtok: {SOME_CONST}"` (a braced reference to a
     script-level constant that is itself `"#654321"`) both satisfy "contains `{`" while being
     exactly as static/theme-invariant as a bare hex — confirmed by construction, injected alone
     into `FileList.svelte` the suite ran 59/59 green with zero mention of the token. Per the
     Foreman's explicit instruction, no third heuristic was attempted. `declaresItself` was deleted
     entirely and replaced with an explicit, closed `LOCAL_CUSTOM_PROPERTIES` allowlist (`--indent`,
     `--sw`), same dated ticket-owning shape as `ALLOWLIST`, each with a one-line note on why it's
     local rather than themed, plus a stale-entry check. `discoverFallbackTokens` now returns
     `{ discovered, allSeen }` — `allSeen` is the pre-live-chain-filter scan, needed because `--sw`'s
     own fallback (`var(--surface-alt)`) is itself a live chain, so `--sw` is already absent from the
     live-chain-filtered `discovered` map by the time the stale-entry check would run against it.
  2. **`stripLineComments`'s quote-aware scanner ate real code.** It had no concept of a regex
     literal — `\/\/` inside `/\/\//` (an ordinary pattern for matching a literal double slash)
     tripped the same `//`-start check and silently deleted the rest of that line, including any
     real `var(--token, <fallback>)` call site sharing it — a false negative, strictly worse than
     the false positive it was fixing. Rewritten per the Foreman's exact spec: strip a line ONLY
     when `//` (optional leading whitespace) is the first thing on it — nothing else inspected, no
     quote state, no regex-literal awareness. A comment-only line can never contain a real call
     site, so it can never eat one; a trailing `// comment` after code is deliberately left alone
     (noise in the safe direction, allowlistable if it ever happens) — the trade-off is stated
     explicitly in the function's own doc comment.

  **Non-blocking note logged per the Foreman's instruction:** a genuinely dynamic declaration written
  as `element.style.setProperty("--tok", computed)` contains no `{` and would have been wrongly
  disqualified under attempt 2's heuristic. `grep -rn "\.style\.setProperty" src/` finds zero matches
  today, and the `LOCAL_CUSTOM_PROPERTIES` replacement makes the point moot regardless (enumeration
  doesn't care how a value is set) — noted for whoever touches this file next.

  **Red-proof evidence** (all four cases run against the real repo, confirmed exactly as required,
  then reverted — `git diff --stat` clean before and after each):
  1. Added `style="--cpe1875r3-braced-string: {'#654321'}"` + `style="--cpe1875r3-braced-const:
     {CPE1875R3_CONST}"` (script-level `const CPE1875R3_CONST = "#654321"`) plus matching
     `var(--cpe1875r3-braced-string, #654321)` / `var(--cpe1875r3-braced-const, #654321)` to
     `FileList.svelte` → both FLAGGED, 5 failures each (all theme blocks), e.g.
     `--cpe1875r3-braced-string (referenced from src/lib/components/FileList.svelte) did not resolve
     to a hex in bare :root (default) — got raw value undefined ... : expected undefined to be
     defined`. Confirmed `--indent`/`--sw` still pass via `LOCAL_CUSTOM_PROPERTIES` throughout
     (60/60 baseline unaffected). Then temporarily removed the `--indent` entry from
     `LOCAL_CUSTOM_PROPERTIES` → 5 failures naming `--indent` (referenced from
     `src/lib/components/PreviewPane.svelte`) in all theme blocks, confirming removal reds.
  2. Added to `src/lib/colorRules.ts`: `const CPE1875R3_R = /\/\//; export const
     CPE1875R3_REGEX_LINE_PROBE = CPE1875R3_R.test("//") ? "var(--cpe1875r3-regexline-probe,
     #654321)" : "";` (an undefined token sharing a line with a regex literal matching `//`) → 5
     failures naming `--cpe1875r3-regexline-probe` in `src/lib/colorRules.ts`, all theme blocks —
     CAUGHT, not swallowed. The original round-1 case (a `//` doc comment alone on its own line
     quoting `var(--cpe1875r3-doccomment-probe, #654321)` by name) — confirmed via a temporary
     `discovered.keys()` dump — was ABSENT from the discovered set: still NOT flagged.
  3. Re-ran the full round-1 probe table against the rewritten code (`calc(var(--cpe1875r3-calc-
     probe, 10px) * 2)`; nested `var(--cpe1875r3-nested-outer, var(--cpe1875r3-nested-inner,
     #fff))`; a REAL multi-line `` var(\n  --cpe1875r3-multiline-probe,\n  #654321\n) `` (backtick
     template literal with actual newlines — an escaped `\n` inside a plain string is literal
     backslash-n text, not a real newline, and was caught as a probe-construction mistake mid-run,
     not a regression, before being corrected); unusual whitespace `var(   --cpe1875r3-whitespace-
     probe   ,   #654321   )`; and a block comment quoting `var(--cpe1875r3-blockcomment-probe,
     #654321)`). Result: all 5 real call sites correctly discovered and flagged (including BOTH
     the nested outer and inner tokens independently — the live-chain exemption still does not
     swallow a chain ending in another undefined token), and the block-comment-only mention
     correctly absent from the discovered set.

  Guardrails re-run clean: `npm run check` — 0 errors, 0 warnings. `npx vitest run` — 331 files /
  4483 tests, all green. Rebasing onto `main` (moved: CPE-1873) and pushing attempt 3.
