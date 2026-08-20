---
id: CPE-1760
title: A filename reaches MediaPlayer's aria-labels raw through a prop, which the render guard cannot see
type: bug
priority: Low
status: Done
tags: ready
estimate: XS
created: 2026-08-15
closed:
---

## Problem

Found by the PR #918 (CPE-1757) round-2 UAT while hunting for another asymmetric-branch miss. It is the
first real instance of a blind spot `bidiRenderScan.ts`'s own header **names in advance**, which is the
point of enumerating them.

`src/lib/components/PreviewPane.svelte:1107` passes `entry.name` **raw** as a prop into `<MediaPlayer>`,
and `MediaPlayer.svelte:131,146` renders it unescaped into two `aria-label={name}` attributes on the audio
and video elements. So a filename carrying a bidi override reaches the accessibility tree in its raw form.

The guard cannot catch this by construction: it scans a registered component's own render positions, and
`{entry.name}` in a **prop** position is legitimate — the value has to travel somewhere. Whether it is safe
depends on what the leaf does with it, and the leaf here does not escape.

## Why Low

It is the **accessibility tree**, not the drawn page — a screen-reader label, not something a sighted user
reads and acts on. The visible surfaces of the same component are escaped. Nothing is written, deleted or
misdirected; a spoofed name is announced in its raw order to a screen reader.

That said, "the user who cannot see the screen gets the unescaped version" is a bad shape to leave standing
once it is known, and the fix is one call.

## The fix, and the more interesting question behind it

Escaping at `MediaPlayer`'s own render is the direct fix — the leaf escapes its own render, which is the
same resolution `DiffSideBySide` already uses and the reason the guard's prop-pass-through exclusion is
sound there.

The general question is worth a moment while you are in the file: **nothing enforces that a leaf escapes.**
The guard's exclusion for prop pass-through is correct only because leaves happen to escape today. Options:
register the leaf components too (cheapest, and `MediaPlayer` should be registered either way), or find a
way to flag a prop whose value derives from a filesystem name and whose leaf does not escape. The first is
almost certainly right; record the reasoning either way.

## Acceptance criteria

- [x] A filename containing a bidi/format character reaches `MediaPlayer`'s `aria-label` in escaped form,
      on both the audio and video elements.
- [x] `MediaPlayer` is in the guard's registry (or its exclusion is recorded with a reason), so a future
      raw render there fails CI.
- [x] A real Arabic and a real Hebrew filename still reach the label unchanged.
- [x] The media element's `src` still carries the **raw** path — the fetch needs the real bytes, exactly as
      `QuickLook`'s `<img src>` does.
- [x] Breaking the escape reds a **distinct** test asserting on the rendered attribute, not on a helper's
      return value.
- [x] If any other leaf component receives a name or path by prop, it gets the same treatment or the same
      recorded exemption — a one-off fix here leaves the class open.

## Resolution (2026-08-19) — stale by the time it was picked up

**The bug this ticket describes was already fixed before this ticket was worked.** PR #937
(`CPE-1766/CPE-1767/CPE-1776/CPE-1768`, merged same day) did the exact widened-net sweep this ticket's
own "more interesting question" section anticipated — it registered `MediaPlayer.svelte` (and
`MediaQuickLook.svelte`) in the guard and, per that PR's own commit message, fixed "MediaPlayer +
MediaQuickLook (media track name, aria-label/title/span)" as part of a 45-file leaf-escaping audit.
Current `MediaPlayer.svelte`:132,147 already read `aria-label={displaySafeName(name)}` on both the
`<video>` and `<audio>` elements; `bidiEscape.guard.test.ts`'s `MediaPlayer.svelte` REGISTRY entry
carries no `s.name`/`name`-shaped raw offender (just non-filesystem UI strings: play/pause/mute labels,
formatted time, playback rate). `MediaQuickLook.svelte` independently escapes its own `aria-label` and
`title` for the same track name (`displaySafeName(track.name)`), and only passes the RAW name onward to
`MediaPlayer` as a prop — which is fine, because the leaf (`MediaPlayer`) escapes it on arrival.

So there was nothing left to fix in `MediaPlayer.svelte` itself. What this pass actually did:

1. **Verified** the fix is real and complete by reading current source + `git show` on the #937 commit
   that introduced it, and by running the full guard suite (`bidiEscape.guard.test.ts` +
   `bidiRenderScan.test.ts`, 79/79 green) plus `MediaPlayer.test.ts`/`MediaQuickLook.test.ts`.
2. **Added the missing red-proof** (this ticket's own AC): `src/lib/components/MediaPlayer.bidiSpoof.test.ts`,
   following the `FileList.bidiSpoof.test.ts`/`DetailsPane.bidiSpoof.test.ts` convention — renders the
   real component and asserts on the rendered `aria-label` ATTRIBUTE (not on `displaySafeName`'s return
   value, which `filename.test.ts` already covers) for both `<audio>` and `<video>`, plus a real Arabic
   and a real Hebrew name reaching the label byte-identical, plus `src` staying raw. Proved it red/green
   the prescribed way: committed the test first, then reverted `aria-label={displaySafeName(name)}` back
   to bare `aria-label={name}` on both elements — the new test failed with `expected '‮gnp.mp4' to
   be '[RLO]gnp.mp4'` (2 of 3 tests red) — then restored via `git checkout --` (safe, since the test was
   already committed) and confirmed green again.
3. **Audited for the broader "leaf receives name/path by prop" class** this ticket's AC #6 asks about:
   grepped every `<Component name={x.name}>`/`<Component path={x.path}>`-shaped call site across
   `src/lib/components/*.svelte` for a receiving prop name outside the guard's own filesystem-identity
   vocabulary (which would make the leaf invisible to `isCandidateComponent`). Found none — every such
   call site passes into a prop literally named `name`/`path`/etc., which the guard's `CANDIDATE_PATTERN`
   already forces into `REGISTRY` (and thus onto the guard's own escaping check) mechanically. The only
   two prop-into-leaf sites carrying a raw filesystem name at all are `PreviewPane.svelte:1107`
   (`name={entry.name}` into `MediaPlayer`) and `MediaQuickLook.svelte:100` (`name={track.name}` into
   `MediaPlayer`) — both land on the now-escaping leaf. Registry baseline was already clean (no
   `s.name`/`name`-style raw offender recorded for `MediaPlayer.svelte` to remove) — nothing was
   laundered into the accepted list here the way WorkbenchView's `fileLabel(f)` was in #937's own re-review.
4. **Judgement call on aria-label's treatment**, as this ticket asked for: kept the bracketed-tag escape
   (`displaySafeName`) rather than inventing a different treatment for `aria-label`. Reasoning: a bidi
   override is a *rendering* instruction — it only does anything to a visual, spatially-laid-out draw of
   the text. A screen reader reads the underlying character stream; it does not re-order glyphs on a
   line the way a browser's bidi algorithm does, so the original "reads as one thing, is actually another"
   spoof mostly doesn't reproduce via speech in the first place. But leaving the raw control character in
   an `aria-label` is still not neutral: some AT/browser stacks vocalize an unexpected format character as
   noise, garbled punctuation, or silently drop it, at the audio-only user's cost, and it is state a
   sighted co-worker glancing at devtools cannot cross-check against what's spoken. `displaySafeName`'s
   bracketed tag (`[RLO]gnp.mp4`) turns that into something a screen reader announces literally and
   informatively ("bracket R L O bracket g n p dot m p 4") — a legible disclosure instead of silent noise
   or a dropped character — and it is the same convention `MediaQuickLook`, `DetailsPane`, `TrashView`,
   etc. already use for their own `aria-label`/`title` pairs, so `aria-label` does not need a bespoke
   treatment; consistency with the rest of the app's AT surface is itself the better outcome for an AT
   user moving between components.

Gates: `npm run check` — 0 errors/warnings. Full frontend suite — 315 files / 4160 tests, all green.

## Notes

Related: CPE-1712 (the original spoof fix), CPE-1757 (PR #918 — the guard, and the header listing this
exact limitation), CPE-1766/CPE-1767/CPE-1776/CPE-1768 (PR #937 — the widened-net sweep that actually
fixed this ticket's bug before it was picked up).
