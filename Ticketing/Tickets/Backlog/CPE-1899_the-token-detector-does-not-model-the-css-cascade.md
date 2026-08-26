---
id: CPE-1899
title: the token detector does not model the CSS cascade, so a base-only `:root` token false-positives in four of five theme blocks
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

CPE-1875's detector requires every referenced token to resolve to a concrete hex in all five theme
blocks (bare `:root`, light, dark, hc-light, hc-dark). `semanticDeclsFor`
(`src/app.css.warn-token.test.ts:128`) extracts declarations strictly from within each theme
selector's own braces.

That is not how CSS works. A value declared once on bare `:root` **inherits** into
`:root[data-theme="..."]` unless that block overrides it. So a token that is legitimately declared
once, without per-theme variants — because it is not a colour, or because one value is correct in
every theme — reads as undefined in four blocks out of five.

Demonstrated by CPE-1875's own reviewer: declaring `--cpe1875probe-cascade-only: #654321` only in the
bare `:root` and referencing it via `var(..., #654321)` passes for "bare :root (default)" and **fails
all four** of light, dark, hc-light and hc-dark.

**Not a live regression today.** `--filelist-cols` (`src/app.css:1327`) is exactly this shape —
declared once, no per-theme override — and escapes only because it happens never to be used with the
`var(token, fallback)` idiom the detector scans for. The trap springs the first time someone extends a
base-only token with a fallback, or adds any new non-colour custom property that does not need per-
theme values. That is an ordinary thing to do, and the developer who does it will get four confusing
failures naming theme blocks they never touched.

## Acceptance criteria

- [ ] Model the cascade: a token defined in bare `:root` and not overridden in a themed block resolves
      to the base value for that block, and must not be reported as missing.
- [ ] Do not weaken the real check while doing it. The failure this detector exists to catch — a token
      defined in light but forgotten in dark — must still fail for dark, and CPE-1875's UAT recorded
      that case as the guard's strongest result. Red-proof it explicitly after the change.
- [ ] Red-proof the new behaviour both ways: a base-only token referenced with a fallback passes in all
      five blocks; a light-only token still fails in the other four.
- [ ] Decide whether a **colour** token should be allowed to be base-only at all, or whether that is
      itself a smell worth flagging separately from a genuinely undefined one. A non-colour custom
      property (a width, a column template) is clearly fine base-only; a colour that is identical in a
      high-contrast theme and a normal one is more questionable. Record the decision at the site.

## Notes

Filed 2026-08-26 by CPE-1875's independent reviewer, which called it out as a design fragility rather
than a defect in that PR and explicitly recommended it be a follow-up rather than folded in — modelling
the cascade is a real change, not a tightening.

Related: **CPE-1875** (the detector), **CPE-1876** (`--mono`, the known-open debt on its allowlist),
**CPE-1810** / **CPE-1821** (the earlier undefined-token defects that motivated the guard).

Note for whoever picks this up: this project's vitest config applies no component CSS under jsdom, so
the detector is a static parse of `src/app.css` and cannot ask a browser what a token resolves to. The
cascade has to be modelled in the parser, not observed. `scripts/dev-harness/` has real-browser
harnesses if a live check ever becomes worth building — but that would be a much larger change than
this ticket.

## A second, related note from CPE-1875's round-3 review (2026-08-26)

Worth folding into the same pass, since both are about how this file decides what to exempt.

**Both allowlists exempt by token NAME globally, not by call site.** `ALLOWLIST` (known-open debt, e.g.
`--mono`) and the new `LOCAL_CUSTOM_PROPERTIES` (`--indent`, `--sw`) are name-lists. If a future
component reused one of those strings for an unrelated, genuinely-global, mis-themed token with a hex
fallback, it would silently inherit the existing entry's exemption.

This is inherent to any name-list allowlist and it predates the change — `--mono` has had the property
since the allowlist was introduced. The surface is narrow: it needs someone to deliberately reuse one of
three specific, unusual names, and any new entry is a diff a human approves. CPE-1875's reviewer
explicitly classified it as **file, don't block** on that basis.

The suggested shape if either list ever grows past a couple of entries: **scope the exemption by file as
well as name**, so an entry covers `--indent` *in `PreviewPane.svelte`* rather than `--indent`
everywhere.

Recorded here rather than as its own ticket because both notes describe the same thing — the file
decides membership by matching text against a list, and the fix in both cases is to make the decision
carry more context.
