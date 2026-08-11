---
id: CPE-1634
title: "62 templated showNotice(`...${value}...`) calls in App.svelte are still untranslated — the half of the notice layer CPE-1627 couldn't reach"
type: Task
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found and honestly reported by the worker on CPE-1627 (PR #825), which converted the **45** raw-literal
`showNotice("...")` calls in `src/App.svelte` to `$t()` across all 12 locale catalogs. A different grep
target — `showNotice(` followed by a **template literal** — turns up **62 more** calls that are still
hardcoded English. They were deliberately left out to keep that PR's already-large diff reviewable.

So the notice layer is currently half localised. A user working in one of the other 11 languages still hits
English at exactly the moments the app is explaining what went wrong — and these 62 are, if anything, the
*more* informative ones, because they are the messages carrying a filename, a count, or a path.

## The work
These are harder than CPE-1627's batch and need real care rather than a mechanical sweep:

- **Interpolation must become placeholders, not concatenation.** A template literal like
  `` `Couldn't copy ${name} to ${dest}.` `` has to become a single keyed string with named placeholders
  substituted by the i18n layer. **Never** assemble a translated sentence from translated fragments — word
  order differs sharply across the 12 supported languages (notably ja/ko/ru/de), and fragment-joining
  produces nonsense in several of them. Follow the placeholder convention the catalogs already use.
- **Watch for plurals.** Any message carrying a count needs whatever plural handling the i18n layer
  supports; several of these languages have plural rules English does not. If the layer has no plural
  support, say so in the work log and pick the least-bad phrasing rather than silently shipping
  "1 files".
- **Some may be trivially convertible** — a backtick string with no interpolation at all is just a literal
  in disguise. Convert those first; they are free.
- Translate every new key across all 12 `COMPLETE_LOCALES` catalogs; the 100%-coverage guard in
  `i18n.test.ts` enforces it.
- **Extend CPE-1627's regrowth guard** (`App.showNoticeI18nGuard.test.ts`) to also fail on a raw templated
  `showNotice`, once these are converted — otherwise the same debt regrows in the form the guard currently
  ignores by design. That extension is the part that makes this stick.

## Two additions from CPE-1627's review (PR #825) — fold these in

**A. A raw literal both greps miss.** `src/App.svelte:2781` has a raw English string inside a **multi-line
ternary**:

    showNotice(
      idx.length === 0 ? "No items match that criterion." : ...

Neither the single-line `showNotice("` grep (which CPE-1627 used) nor the template-literal grep catches it,
so it falls through **both** tickets' stated scopes. It is pre-existing and, as far as the reviewer could
find, the only instance — but "as far as we could find with a line-based grep" is exactly the weakness here.
Prefer an **AST/parser-based sweep** over another regex when doing this pass, so multi-line and nested
expression forms cannot hide.

**B. The regrowth guard's escape hatch is trivially defeatable — demonstrated.** CPE-1627 added
`App.showNoticeI18nGuard.test.ts`, whose exemption check is a bare substring test
(`!line.includes("i18n-exempt")`) rather than being comment-syntax-aware. The reviewer defeated it by
embedding the marker **inside the user-facing string itself**:

    showNotice("This is real English user-facing text i18n-exempt smuggled in string")

The guard passed, and a genuinely untranslated notice would ship. Harden it to require an actual `//`
comment prefix (and ideally a non-empty reason). Not urgent — the hatch is currently unused, with zero
exemptions in the shipped code — but a guard that can be bypassed by accident is worse than one that can't,
because it is trusted.

## Acceptance criteria
- No untranslated `showNotice` call of either form remains in `src/App.svelte`; all 12 catalogs at 100%.
- Interpolated values render correctly and in a natural position in at least ja, ru, de and es — verified by
  a test that renders in a non-English locale, not by inspection.
- The regrowth guard covers both raw-literal and templated forms, and fails when a new one is added
  (demonstrate the failure — a guard that can't fail is worse than none).
- No behavioural change: every notice fires under the same conditions, with the same second argument, and
  dismisses the same way.

**Conflict surface:** `src/App.svelte`, `src/lib/i18n.ts` (12 catalogs), `src/App.showNoticeI18nGuard.test.ts`.
Large and heavily concentrated in `App.svelte` — **do not run in parallel with other `App.svelte` work**, and
land CPE-1627 (PR #825) first.
