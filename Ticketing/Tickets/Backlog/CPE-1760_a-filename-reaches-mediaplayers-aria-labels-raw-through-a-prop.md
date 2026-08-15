---
id: CPE-1760
title: A filename reaches MediaPlayer's aria-labels raw through a prop, which the render guard cannot see
type: bug
priority: Low
status: Backlog
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

- [ ] A filename containing a bidi/format character reaches `MediaPlayer`'s `aria-label` in escaped form,
      on both the audio and video elements.
- [ ] `MediaPlayer` is in the guard's registry (or its exclusion is recorded with a reason), so a future
      raw render there fails CI.
- [ ] A real Arabic and a real Hebrew filename still reach the label unchanged.
- [ ] The media element's `src` still carries the **raw** path — the fetch needs the real bytes, exactly as
      `QuickLook`'s `<img src>` does.
- [ ] Breaking the escape reds a **distinct** test asserting on the rendered attribute, not on a helper's
      return value.
- [ ] If any other leaf component receives a name or path by prop, it gets the same treatment or the same
      recorded exemption — a one-off fix here leaves the class open.

## Notes

Related: CPE-1712 (the original spoof fix), CPE-1757 (PR #918 — the guard, and the header listing this
exact limitation).
