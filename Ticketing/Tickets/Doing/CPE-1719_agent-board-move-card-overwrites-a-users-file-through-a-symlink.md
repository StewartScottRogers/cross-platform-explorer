---
id: CPE-1719
title: The board sidecar's move_card writes through a symlink and destroys the user's unrelated file
type: bug
priority: High
status: In Progress
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

## Work Log

**2026-08-13 — picked up, branch `cpe-1719-board-write-through-link`.**

### The fix

`board::write_slot_refusal` + the pure `board::classify_write_slot`, called from `move_card` immediately
before `fs::write` and only when `dest != src`. One `fs::symlink_metadata` answers the whole three-state
question for a *write* slot, because it never follows the final component:

| `symlink_metadata` | verdict |
|---|---|
| `Ok(true)` — a link (live **or** dangling) | refuse, naming the link |
| `Ok(false)` — a real entry | refuse, "a file already exists at …" |
| `Err(NotFound)` | **free** — the only answer that means free |
| `Err(other)` | refuse; "refusing to guess", deliberately never says "already exists" |

`Path::try_exists` is not used at all here. It follows links, so it answers `Ok(false)` for a dangling one
— it structurally cannot see the case that motivated the ticket.

**Order:** the link verdict comes first, the opposite of `fsutil::rename_slot_refusal`, which puts the
clobber check first to preserve wording its sites already shipped. This site has no such history, and a
live link would answer "the name is taken" to an occupancy check too — link-first is what makes a live
link report *that it is a link*.

### The dangling case — decided: refused

`fs::write` through a dangling link **creates** the target, so the board would materialise a file at a path
the user never named, anywhere on disk the link points, while the card appeared to be sitting in the
column. Nothing about "the far end is empty" makes the far end ours. Same arm as a live link, refused
identically, and `a_dangling_link_at_the_destination_is_refused_and_its_target_is_not_created` asserts the
would-be target was not conjured into being.

### `src == dest` is exempt, on purpose

That is the legitimate no-op move — the card is already in this column and all that happens is its own
`status:` being rewritten in place. Exempt even when the ticket file is itself a link, because then the far
end *is* the ticket: the very bytes `read_to_string` returned two lines earlier. Refusing would make a
symlinked ticket unmovable rather than safer. Held to its terms by
`a_no_op_move_still_rewrites_the_status_in_place`.

### Enumeration of this crate's destructive primitives

`grep` over `sidecar/agent-board/src/**` for `fs::write`, `fs::copy`, `OpenOptions`, `File::create`,
`remove_file`, `remove_dir_all`, `fs::rename`. Outside `#[cfg(test)]` the crate has exactly **two**:

| Site | Primitive | Hazard | Status |
|---|---|---|---|
| `board.rs` `move_card` destination | `fs::write` | follows the final component ⇒ writes **through** a link and destroys an unrelated file | **fixed** — `write_slot_refusal` |
| `board.rs` `move_card` source | `fs::remove_file` | removes the **link**, not its target ⇒ a link the user made is removed and their file left orphaned with stale content | **documented, not refused** — data orphaned, not destroyed, and a move must remove its source |

`fs::copy`, `OpenOptions` and `File::create` do not appear in this crate at all. `ui.rs`'s three `fs::write`
calls are all inside its `#[cfg(test)]` module (line 301 opens it; the writes are at 309/387/397). There is
no `fs::rename` here, which is why CPE-1710's lint could not have caught this even once the `clippy.toml`
it adds lands.

**A hard link at the destination is also covered**, by the occupied arm rather than the link arm — the
link check cannot see one (`is_symlink()` is false) but `symlink_metadata` still succeeds, so the name
reads as taken. That matters on Windows, where a hard link needs no privilege and truncating through one
destroys the victim exactly the way the reported symlink did.

### The twins — checked, both clean

- **In-process `board_move_impl`** (`src-tauri/src/lib.rs:131`) does **not** have this shape. It writes to
  `src` (`fs::write(&src, set_status(&md, status))` at :166) and then `fs::rename`s to `dest`. The write
  target is the file the content was just read from, so no unrelated file is reachable; the destination is
  a rename, already guarded by `clobber_refusal` and gaining the symlink half from PR #895.
- **`ticket_mcp`** (`crates/server/src/ticket_mcp.rs`) has **no** destructive primitive at all — the same
  grep returns nothing. It is a read-only view over the folders.

### No `clippy.toml` change here

PR #895 (CPE-1710) creates `sidecar/agent-board/clippy.toml` in flight; adding a second copy on this branch
would conflict for no benefit, and its `disallowed-methods` entry is for `std::fs::rename`, which this crate
does not call. Extending the lint to `fs::write` was considered and **rejected as written**: `clippy
--all-targets` covers test code, where this crate alone has seven legitimate fixture writes, so the entry
would cost more `#[allow]`s than it would buy signal. A narrower mechanism is worth a separate ticket, not
a hasty entry here.

### Evidence

Guard neutralisation, each break made on its own and restored with `git checkout --` (never a copy — a
restored backup's older timestamp leaves `cargo` believing the broken build is current). Real output is
pasted in the PR body.
