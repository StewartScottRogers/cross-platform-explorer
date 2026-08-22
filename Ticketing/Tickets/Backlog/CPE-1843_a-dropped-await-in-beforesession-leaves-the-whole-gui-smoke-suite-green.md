---
id: CPE-1843
title: a dropped await in beforeSession would leave the whole gui-smoke suite green
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-21
closed:
---

## Problem

`gui-smoke/wdio.conf.ts`'s `beforeSession` waits for two ports before returning — tauri-driver's own
port and the native driver's. Both waits are correctly `await`ed today (`:1304` and `:1307-1313`,
verified by reading the merged code).

**Nothing pins that.** `npm run test:unit` exercises the extracted `waitForPort` function in isolation
(`gui-smoke/lib/waitForPort.test.ts`); nothing exercises `beforeSession` itself. A future edit that drops
one `await` — the classic form of exactly this bug — would leave the full 121-test suite green, and the
race would return silently on the platform where it is hard to reproduce.

That is the same shape this whole ticket family keeps closing: the fix is right, and the thing that keeps
it right is untested.

## A second, related exposure

`.github/workflows/gui-smoke.yml:202` and `:463` run `cargo install tauri-driver --locked` with **no
version pin**, so CI installs whatever is newest on crates.io.

CPE-1832's fix depends on `--port` and `--native-port` remaining real flags with stable defaults —
verified against tauri-driver 2.0.6's `cli.rs` (4444 and 4445, matching the harness constants exactly).
If upstream renames or re-defaults them, the harness silently stops waiting on the right thing.

Pre-existing, not introduced by CPE-1832 — but that fix increased how much depends on that CLI shape
staying still. Pinning the version makes an upstream break show up as a deliberate version bump rather
than a CI mystery.

## Acceptance criteria

- [ ] A regression guard fails if either `waitForPort` call in `beforeSession` loses its `await`. A
      lint rule against un-awaited calls, a type-level guard, or a test that exercises `beforeSession`
      directly — whichever is cheapest to keep honest.
- [ ] Red-proof it: drop one `await`, confirm the guard fires, restore, confirm green. If the guard does
      not fire, it has not closed the gap.
- [ ] `tauri-driver` is version-pinned in both workflow sites, or the decision not to pin is recorded
      with its reason.
- [ ] If pinned: a note next to the pin saying what depends on it (the `--port` / `--native-port` flags
      and their 4444/4445 defaults), so the next person bumping it knows what to re-check.

## Notes

Filed from the CPE-1832 review, which classified all of this as FOLLOW-UP rather than blocking — the
code is correct today and the fix was verified green on all four Linux CI shards, including shard 2 where
the original failure was observed.

A third observation from that review, recorded but not actioned: shard 2 took ~14 minutes against 6-7 for
the others before landing green. Nothing in the fix explains it (the two-port wait is bounded to about 30
seconds worst case), so it is almost certainly spec-assignment variance — worth a glance only if it
recurs.
