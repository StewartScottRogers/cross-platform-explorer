---
id: CPE-1828
title: the degraded Trash notes are mounted already containing their text, so a screen reader may never announce them
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

CPE-1816 added `role="status"` to both `.tv-degraded-note` occurrences, and it works in the sense that
the role is present on all three degraded shapes (with-entries banner, empty-trash note, and the
drained-mid-stream note). But both notes are **freshly mounted already containing their text**, which is
the unreliable live-region shape: a node inserted into the DOM with its content in the same mutation is
frequently not announced at all. Chromium + Windows AT — i.e. **WebView2 with NVDA or Narrator, exactly
this app's combination** — is the weakest pairing for it.

The title-bar slot CPE-1816 built is the *correct* shape by contrast: the same DOM node exists at mount
and its text mutates in place (`"" → "Still loading…" → "3 items"`), so the mid-stream case — that
ticket's actual scope — is announced reliably.

The consequence is only at the **degraded exit**: the stale "Still loading…" is withdrawn by the slot
going to `""`, which announces nothing on its own, and the degraded note that replaces it may or may not
be announced. Net today is *best-effort* announcement rather than guaranteed silence — strictly better
than before CPE-1816, never worse.

## Acceptance criteria

- [ ] The degraded text is announced reliably: either hoist a **persistent** wrapper element carrying
      `role="status"` that exists from mount and has its text mutated, or drive the degraded message
      through the title-bar region CPE-1816 already built correctly.
- [ ] All three degraded shapes are covered — with-entries, empty-trash, and drained-mid-stream.
- [ ] The mid-stream → degraded transition withdraws the earlier claim rather than going silent.
- [ ] Verify with a real screen reader against the installed build, not by reading markup — this is a
      behaviour of the AT, not of the DOM, and the whole point of the ticket is that the markup looks
      fine.
- [ ] Fix the code comment CPE-1816 left behind. It asserts that fresh-mount is "the correct live-region
      shape for genuinely NEW information", which inverts the accepted guidance and contradicts the
      reasoning applied correctly to the title-bar slot in the same file.

## Notes

Filed from the CPE-1816 Visual Critic's round-3 a11y finding, which it explicitly classified as a
follow-up rather than a merge blocker. The verification step needs a real screen reader, so this is a
candidate row for the QA Architect's manual-test burndown if it cannot be automated.
