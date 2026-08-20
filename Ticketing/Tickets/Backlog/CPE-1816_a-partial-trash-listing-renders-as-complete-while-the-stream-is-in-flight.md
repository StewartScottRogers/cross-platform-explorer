---
id: CPE-1816
title: a partial trash listing renders as complete while the stream is still in flight
type: bug
priority: Low
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

While a trash listing is **still streaming**, rows paint as batches arrive and the title bar asserts
"Trash 1 item", "Trash 2 items" — **with no incompleteness notice**. The notice and the count-suppression
appear only when the stream's summary resolves.

So for the duration of the stream, a partial list renders exactly as a complete one would.

## Why it is not simply a bug in CPE-1803/CPE-1804

It is **inherent to the streaming design**: the `degraded` / `skipped` flags ride on the *summary*, which by
construction arrives last. Entries stream over the channel; the verdict does not exist until the walk ends.
Unchanged since CPE-1560 and untouched by CPE-1803 and CPE-1804 — both of which fixed the *completed* case
correctly.

The window closes on completion, so this is materially milder than the bugs those tickets fixed: the user
is briefly under-informed rather than durably misinformed. On a small trash it is imperceptible.

## Why it is still worth fixing

The harm scales with the thing that makes streaming worth having. A large or slow trash — a network mount,
a spinning disk, thousands of entries — is exactly when the window is widest **and** when the user is most
likely to read a partial list and act on it. "Your file isn't in the trash" is a conclusion someone can
reach in two seconds from a list that is still filling.

## What to do

- The honest shape is probably to **let the stream carry incompleteness as it happens** rather than only in
  the summary — the walker knows it skipped an entry at the moment it skips it. Weigh sending it with the
  batch against the simplicity the current design buys, and **say why** either way.
- Failing that, consider suppressing the *count* until the summary resolves, so the app at least never
  asserts a total it cannot yet back. That is strictly weaker but much cheaper.
- Whatever the shape, keep the property CPE-1804 established: the notice must be driven by the flag, never
  inferred from `entries.length`.
- Follow the streaming standard in [docs/design/STREAMING.md](../../../docs/design/STREAMING.md), including
  supersession by generation token — a fix here must not resurrect a stale banner over a fresh listing.

## Evidence

Per the Evidence Rules in `Ticketing/wiki.md`. The test must observe the **mid-stream** state, not just the
resolved one — a test that only asserts the final render passes today and would pass with this unfixed.

## Notes

Filed by the Foreman from PR #962's UAT, 2026-08-20. The UAT observed the timeline directly rather than
inferring it, and correctly classified it as pre-existing rather than a regression in the PR it was testing.

Related: **CPE-1803**, **CPE-1804**, **CPE-1805**, **CPE-1560**.

## Work Log

**2026-08-20** — Implemented the "failing that" fallback the ticket sanctioned: suppress the count
rather than carry `skipped` on every batch. Reasoning for taking the cheaper route: `degraded`/`skipped`
are folded from a caught-panic route (which by definition has no per-item information to send early)
and a per-item skip route — a shared per-batch signal would need to model "maybe-degraded-so-far" as a
third, meaningfully different state from `degraded`, adding a new wire shape (batch envelope change,
`specta::Type` touch, bindings regen, all 5 command call sites in `stream_trash_entries`) to close a
window that is inherently transient and self-corrects the moment the summary lands — CPE-1804/CPE-1805
already made the *resolved* state fully honest, which is the state that persists.

Reused the exact mechanism CPE-1805 built (the banner above the rows) rather than inventing a second
one: added one frontend-only `complete` boolean (true once the stream's summary resolves or the invoke
throws), and drive both the title-bar item-count suppression and the banner from `degraded || !complete`
— `degraded` wins once it's known (it's never true before `complete` is), otherwise a new
`trash.stillLoading` string ("Still loading…") reuses the same box and position CPE-1805 introduced.
No backend change; no `specta::Type` touched; no bindings regen needed.

Added `trash.stillLoading` to all 12 complete locales in `src/lib/i18n.ts` (en/es/de/fr/it/pt/nl/pl/
ru/zh/ja/ko), following the `trash.skippedOne`/`skippedMany` precedent from CPE-1804. Updated
`src/docs/38-trash.md` to describe the mid-stream state. Updated two stale-line guard tests whose
recorded line numbers shifted (`bidiEscape.guard.test.ts`'s `TrashView.svelte` REGISTRY entry,
`mojibakeGuard.test.ts`'s i18n.ts `ALLOWLIST` entry for the Portuguese "NÃO" line) — both re-anchored
on content, not arithmetic, same as prior tickets' notes on those two guards.

New test `does not assert a finished item count while the stream is still in flight, and says so`
observes the mid-stream state directly (delivers a batch, does NOT resolve the summary) per the
ticket's Evidence Rules — a suite asserting only the resolved state would pass today and would still
pass with the bug intact.

Red-proofs, each applied, observed red, then reverted:
- Removed `&& complete` from the title-bar count's `{#if}` (`TrashView.svelte`, was
  `!loading && !error && !degraded && complete`) → the new test reds on
  `expect(screen.queryByText("1 item")).toBeNull()` (found a live `<span>1 item</span>` instead).
- Changed the rows-banner `{#if degraded || !complete}` to `{#if degraded}` → the same test reds on
  `expect(screen.getByText("Still loading…")).toBeTruthy()` (element not found).

Gates: `npm run check` — 0 errors, 0 warnings. `npx vitest run` — 4183 passed (4183), 0 failed (was
4182 on main; +1 from the new CPE-1816 test). No `.rs` files touched (frontend-only fix), so no cargo
gates or bindings regen apply this round.

Files changed: `src/lib/components/TrashView.svelte`, `src/lib/components/TrashView.test.ts`,
`src/lib/i18n.ts`, `src/lib/bidiEscape.guard.test.ts`, `src/lib/mojibakeGuard.test.ts`,
`src/docs/38-trash.md`.
