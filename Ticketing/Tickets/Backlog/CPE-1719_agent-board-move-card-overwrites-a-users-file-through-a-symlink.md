---
id: CPE-1719
title: The board sidecar's move_card writes through a symlink and destroys the user's unrelated file
type: bug
priority: High
status: Backlog
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Found by the PR #895 (CPE-1710) UAT, 2026-08-13. **Measured, not inferred** — driven through the real
`move_card` with a live symlink in `Ticketing/Tickets/Doing/` pointing at an unrelated user file:

```
[UAT] move_card returned: Ok("Doing")
[UAT] slot is still a link: true
[UAT] victim bytes now: "---\nid: CPE-9999\nstatus: In Progress\n---\n\nbody\n"
assertion failed: the user's unrelated file was OVERWRITTEN through the link by a board move
  left:  "---\nid: CPE-9999\nstatus: In Progress\n---\n\nbody\n"
  right: "MY NOTES"
```

`sidecar/agent-board/src/board.rs:398` (`move_card`) has **no destination guard at all**, and its
destructive primitive is `fs::write`, which **follows** the final path component. So:

1. The user's unrelated file is **overwritten** with ticket frontmatter.
2. The **link survives**, so the board looks completely normal afterwards.
3. The source card is **deleted**.
4. The call returns **`Ok`**.

## Why this is worse than the sites CPE-1710 fixed

Those destroy a *link* — annoying, recoverable, and once guarded they fail loudly. This one destroys **the
contents of a file the user never named**, leaves the board looking healthy, and reports success. There is
no signal at any point that anything went wrong.

## This is the sidecar twin of a site CPE-1710 just fixed

`board_move_impl` — the in-process board move — is one of the six sites PR #895 guards. `move_card` is the
**same user-visible operation** in the standalone sidecar. Per the standing rule that *both board
implementations change in lockstep* (the in-process `crates/server` + `src-tauri` one, the
`sidecar/agent-board` crate, and the ticket MCP all read the same folders), it should have moved with it.

It did not, for two structural reasons worth recording:

- **Different primitive.** CPE-1710's guard and its clippy lint are both about `fs::rename`. `fs::write`
  follows a link and writes *through* it, which is a different failure with a different fix.
- **Uncovered root.** `sidecar/agent-board` is one of the eleven workspace roots without a `clippy.toml`,
  so the lint could not have caught it even if the primitive matched.

## Scope

`sidecar/agent-board/src/board.rs`'s `move_card`. Check its siblings in the same file at the same time — if
one write path has this shape, its neighbours probably do.

## Acceptance criteria

- [ ] A symlink at the destination slot cannot be written through. Either refuse and say why, or resolve
      deliberately — record the choice.
- [ ] The **dangling** link case is handled too, and stated. `fs::write` through a dangling link creates the
      target; decide whether that is acceptable and say so.
- [ ] A test proves it, asserting on **the victim file's bytes** and on the slot still being a symlink —
      never on the returned `Result`, which was `Ok` throughout this bug.
- [ ] **Enumerate the sidecar's other destructive primitives** rather than fixing only `move_card`.
      `fs::write`, `fs::copy`, `OpenOptions::create`, and `remove_file` all have destination hazards, and
      none is covered by CPE-1710's rename lint.
- [ ] Check the in-process board and the ticket MCP for the same shape. The lockstep rule cuts both ways —
      if the sidecar had this, confirm its twins do not.
- [ ] Platform-gate correctly. Creating a symlink on Windows needs Developer Mode or elevation; a test that
      silently no-ops on an unprivileged runner proves nothing. `fsutil::make_dangling_link`'s junction
      fallback is the pattern, and note it lives in `cpe-server` — the sidecar cannot reach it.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #895 UAT, 2026-08-13, which correctly scoped it out of that PR — different
primitive, different crate — and measured it rather than reporting it as a suspicion.

Related: **CPE-1710** (which fixed the in-process twin and whose lint cannot reach here), **CPE-1716** and
**CPE-1718** (the other sites this family's enumeration surfaced), **CPE-1717** (skip notices invisible in
CI, which matters for whatever platform-gated test this ticket adds).
