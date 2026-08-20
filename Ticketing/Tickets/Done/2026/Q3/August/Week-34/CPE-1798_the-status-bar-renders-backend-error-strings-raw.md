---
id: CPE-1798
title: the status bar renders backend error strings raw, from 35 call sites
type: bug
priority: Low
status: Done
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

`src/lib/components/StatusBar.svelte:96` renders its `notice` prop raw in **two** render positions:

```svelte
<span class="notice" class:error={noticeIsError} title={notice}>{notice}</span>
```

`App.svelte` feeds it backend error strings from **35** distinct `showNotice(String(e), true)` call
sites, plus `:1245` (`"Sync failed: " + e.message`). Those are Rust errors, and they routinely embed the
offending path — so a filesystem name reaches both the visible text and the tooltip unescaped.

The render guard does not see it. `StatusBar` takes a generically-named `notice` prop and contains no
`.name`/`.path`-shaped identifier of its own, so `isCandidateComponent` never matches. It is the same
structural gap CPE-1790 closed for the confirm dialogs, one component to the left.

## Why it was recorded as out of scope, and why that needs correcting

`bidiRenderScan.ts`'s "Out of scope" example names `StatusBar`, and that example is **correct as
written** — it describes a *textual-shape* criterion, and `StatusBar` matches no shape. The problem is
the stronger runtime claim made in PR #949's description: that the swept generic-prop components
including `StatusBar` are "fed only static UI copy today — no live gap". They are not. The 35 error
paths are live.

That claim has been dropped from the PR body. This ticket is the follow-up it pointed at.

## Consequence, honestly scoped

Lower than a decision surface. A confirm dialog asks you to authorise something and a spoofed name
there can make you approve the wrong action; a status bar reports what already happened. But it is
still a filesystem name rendered to a person in a place they read to understand what went wrong, and a
`title=` tooltip is one of the three positions the guard treats as a render position precisely because
it is read.

## What to do

- Escape on arrival in the leaf, matching the model CPE-1790 established: `displaySafeName` /
  `displaySafePath` on `notice` at both render positions. That is two lines and, thanks to CPE-1790's
  new candidacy trigger, it makes `StatusBar` self-registering — a component that calls the escape
  helper is by construction a candidate, so it gains a `REGISTRY` entry and is pinned from then on.
- Check the other generically-named-prop components swept during CPE-1790 with the same runtime
  question rather than the shape question: `AgentMenu`, `HelpButton`, `JsonTree`, `Submenu`, `Toolbar`
  were cleared as static UI copy — verify that against their actual call sites, since `StatusBar` was
  cleared the same way and was not.
- **Red-proof it**: a notice composed from a path containing a bidi override must render the escape
  marker, and removing the wrap must fail the guard. Per the Evidence Rules in `Ticketing/wiki.md`.
- Consider whether escaping a whole error *message* (rather than a bare name) mangles anything
  legitimate. CPE-1790's reviewer scanned every `.ts`/`.json`/`.svelte`/`.md` under `src/` for literal
  bidi/format characters and found **zero**, including the locale files — so there is no known
  legitimate content to damage, but re-check if that changes.

## Notes

Found by the independent reviewer of PR #949 (CPE-1790), 2026-08-19, while checking whether that PR's
generic-prop sweep had cleared components on the right basis. It had not for this one.

Related: **CPE-1790** (the confirm dialogs, same structural gap), **CPE-1768** (the membership rule),
**CPE-1760** (the leaf-escapes-on-arrival model).
