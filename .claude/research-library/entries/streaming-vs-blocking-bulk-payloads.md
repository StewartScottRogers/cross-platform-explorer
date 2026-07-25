---
topic:       streaming-vs-blocking-bulk-payloads
title:       Should large/slow producers stream in batches or return one blocking Vec?
date:        2026-07-25
researcher:  opus
relates:     [epic-662]
tags:        [streaming, tauri, ipc-channel, performance, liveness, directory-listing, search]
status:      current
sources:     [in-repo, worktree-probe]
---

## Question
Directory listings, recursive searches, and future bulk producers can be large or slow. Should the
backend collect the whole result and return it from a single `invoke` (simple), or stream it in batches
over a Tauri `ipc::Channel` (live)? What's the standard the whole app should follow?

## Findings / Options
- **A — Blocking `invoke` returning one `Vec`.** Simplest to write and test; one round-trip. But the
  pane paints nothing until the *entire* walk finishes, so a big/slow directory or a deep recursive
  search feels frozen. Cost grows with payload size — the worst case is exactly when the user is
  watching hardest.
- **B — Stream batches over an `ipc::Channel`.** Backend emits rows in chunks; frontend appends each
  batch, flips `loading` off on the **first** batch, and supersedes an in-flight stream by generation
  token. First rows appear immediately regardless of total size. More moving parts (channel plumbing,
  generation tokens, batch cadence) and slightly harder to unit-test.
- **Shared-walker refinement.** One walker backs **both** a collect-to-vec command *and* its streaming
  variant, so the two don't diverge — you get the simple API for callers that want a whole list and the
  live API for panes, from a single implementation.

## Recommendation
**B, via the shared-walker refinement.** Perceived liveness on the exact large/slow cases the explorer
hits daily outweighs the extra plumbing, and backing both APIs with one walker keeps the simplicity
cost bounded. Codified as the repo standard: producers of large/slow payloads stream in batches; new
bulk producers follow it. (This entry catalogues an already-settled decision as the Library's seed.)

## Sources
- `docs/design/STREAMING.md` — the ratified standard.
- `CLAUDE.md` → "Streaming liveness" UI convention.
- Memory `[[prefer-streaming-liveness]]` (epic CPE-662).
