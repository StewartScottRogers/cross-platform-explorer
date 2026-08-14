---
id: CPE-1719
title: The board sidecar's move_card writes through a symlink and destroys the user's unrelated file
type: bug
priority: High
status: Done
tags: ready
estimate: S
created: 2026-08-13
closed: 2026-08-13
---

## Problem

Found by the PR #895 (CPE-1710) UAT, 2026-08-13. **Measured, not inferred** â€” driven through the real
`move_card` with a live symlink in `Ticketing/Tickets/Doing/` pointing at an unrelated user file:

```
[UAT] move_card returned: Ok("Doing")
[UAT] slot is still a link: true
[UAT] victim bytes now: "---\nid: CPE-9999\nstatus: Done\n---\n\nbody\n"
assertion failed: the user's unrelated file was OVERWRITTEN through the link by a board move
  left:  "---\nid: CPE-9999\nstatus: Done\n---\n\nbody\n"
  right: "MY NOTES"
```

`sidecar/agent-board/src/board.rs:398` (`move_card`) has **no destination guard at all**, and its
destructive primitive is `fs::write`, which **follows** the final path component. So:

1. The user's unrelated file is **overwritten** with ticket frontmatter.
2. The **link survives**, so the board looks completely normal afterwards.
3. The source card is **deleted**.
4. The call returns **`Ok`**.

## Why this is worse than the sites CPE-1710 fixed

Those destroy a *link* â€” annoying, recoverable, and once guarded they fail loudly. This one destroys **the
contents of a file the user never named**, leaves the board looking healthy, and reports success. There is
no signal at any point that anything went wrong.

## This is the sidecar twin of a site CPE-1710 just fixed

`board_move_impl` â€” the in-process board move â€” is one of the six sites PR #895 guards. `move_card` is the
**same user-visible operation** in the standalone sidecar. Per the standing rule that *both board
implementations change in lockstep* (the in-process `crates/server` + `src-tauri` one, the
`sidecar/agent-board` crate, and the ticket MCP all read the same folders), it should have moved with it.

It did not, for two structural reasons worth recording:

- **Different primitive.** CPE-1710's guard and its clippy lint are both about `fs::rename`. `fs::write`
  follows a link and writes *through* it, which is a different failure with a different fix.
- **Uncovered root.** `sidecar/agent-board` is one of the eleven workspace roots without a `clippy.toml`,
  so the lint could not have caught it even if the primitive matched.

## Scope

`sidecar/agent-board/src/board.rs`'s `move_card`. Check its siblings in the same file at the same time â€” if
one write path has this shape, its neighbours probably do.

## Acceptance criteria

- [ ] A symlink at the destination slot cannot be written through. Either refuse and say why, or resolve
      deliberately â€” record the choice.
- [ ] The **dangling** link case is handled too, and stated. `fs::write` through a dangling link creates the
      target; decide whether that is acceptable and say so.
- [ ] A test proves it, asserting on **the victim file's bytes** and on the slot still being a symlink â€”
      never on the returned `Result`, which was `Ok` throughout this bug.
- [ ] **Enumerate the sidecar's other destructive primitives** rather than fixing only `move_card`.
      `fs::write`, `fs::copy`, `OpenOptions::create`, and `remove_file` all have destination hazards, and
      none is covered by CPE-1710's rename lint.
- [ ] Check the in-process board and the ticket MCP for the same shape. The lockstep rule cuts both ways â€”
      if the sidecar had this, confirm its twins do not.
- [ ] Platform-gate correctly. Creating a symlink on Windows needs Developer Mode or elevation; a test that
      silently no-ops on an unprivileged runner proves nothing. `fsutil::make_dangling_link`'s junction
      fallback is the pattern, and note it lives in `cpe-server` â€” the sidecar cannot reach it.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #895 UAT, 2026-08-13, which correctly scoped it out of that PR â€” different
primitive, different crate â€” and measured it rather than reporting it as a suspicion.

Related: **CPE-1710** (which fixed the in-process twin and whose lint cannot reach here), **CPE-1716** and
**CPE-1718** (the other sites this family's enumeration surfaced), **CPE-1717** (skip notices invisible in
CI, which matters for whatever platform-gated test this ticket adds).

## Work Log

**2026-08-13 â€” picked up, branch `cpe-1719-board-write-through-link`.**

### The fix

`board::write_slot_refusal` + the pure `board::classify_write_slot`, called from `move_card` immediately
before `fs::write` and only when `dest != src`. One `fs::symlink_metadata` answers the whole three-state
question for a *write* slot, because it never follows the final component:

| `symlink_metadata` | verdict |
|---|---|
| `Ok(true)` â€” a link (live **or** dangling) | refuse, naming the link |
| `Ok(false)` â€” a real entry | refuse, "a file already exists at â€¦" |
| `Err(NotFound)` | **free** â€” the only answer that means free |
| `Err(other)` | refuse; "refusing to guess", deliberately never says "already exists" |

`Path::try_exists` is not used at all here. It follows links, so it answers `Ok(false)` for a dangling one
â€” it structurally cannot see the case that motivated the ticket.

**Order:** the link verdict comes first, the opposite of `fsutil::rename_slot_refusal`, which puts the
clobber check first to preserve wording its sites already shipped. This site has no such history, and a
live link would answer "the name is taken" to an occupancy check too â€” link-first is what makes a live
link report *that it is a link*.

### The dangling case â€” decided: refused

`fs::write` through a dangling link **creates** the target, so the board would materialise a file at a path
the user never named, anywhere on disk the link points, while the card appeared to be sitting in the
column. Nothing about "the far end is empty" makes the far end ours. Same arm as a live link, refused
identically, and `a_dangling_link_at_the_destination_is_refused_and_its_target_is_not_created` asserts the
would-be target was not conjured into being.

### `src == dest` is exempt, on purpose

That is the legitimate no-op move â€” the card is already in this column and all that happens is its own
`status:` being rewritten in place. Exempt even when the ticket file is itself a link, because then the far
end *is* the ticket: the very bytes `read_to_string` returned two lines earlier. Refusing would make a
symlinked ticket unmovable rather than safer. Held to its terms by
`a_no_op_move_still_rewrites_the_status_in_place`.

### Enumeration of this crate's destructive primitives

`grep` over `sidecar/agent-board/src/**` for `fs::write`, `fs::copy`, `OpenOptions`, `File::create`,
`remove_file`, `remove_dir_all`, `fs::rename`. Outside `#[cfg(test)]` the crate has exactly **two**:

| Site | Primitive | Hazard | Status |
|---|---|---|---|
| `board.rs` `move_card` destination | `fs::write` | follows the final component â‡’ writes **through** a link and destroys an unrelated file | **fixed** â€” `write_slot_refusal` |
| `board.rs` `move_card` source | `fs::remove_file` | removes the **link**, not its target â‡’ a link the user made is removed and their file left orphaned with stale content | **documented, not refused** â€” data orphaned, not destroyed, and a move must remove its source |

`fs::copy`, `OpenOptions` and `File::create` do not appear in this crate at all. `ui.rs`'s three `fs::write`
calls are all inside its `#[cfg(test)]` module (line 301 opens it; the writes are at 309/387/397). There is
no `fs::rename` here, which is why CPE-1710's lint could not have caught this even once the `clippy.toml`
it adds lands.

**A hard link at the destination is also covered**, by the occupied arm rather than the link arm â€” the
link check cannot see one (`is_symlink()` is false) but `symlink_metadata` still succeeds, so the name
reads as taken. That matters on Windows, where a hard link needs no privilege and truncating through one
destroys the victim exactly the way the reported symlink did.

### The twins â€” checked, both clean

- **In-process `board_move_impl`** (`src-tauri/src/lib.rs:131`) does **not** have this shape. It writes to
  `src` (`fs::write(&src, set_status(&md, status))` at :166) and then `fs::rename`s to `dest`. The write
  target is the file the content was just read from, so no unrelated file is reachable; the destination is
  a rename, already guarded by `clobber_refusal` and gaining the symlink half from PR #895.
- **`ticket_mcp`** (`crates/server/src/ticket_mcp.rs`) has **no** destructive primitive at all â€” the same
  grep returns nothing. It is a read-only view over the folders.

### No `clippy.toml` change here

PR #895 (CPE-1710) creates `sidecar/agent-board/clippy.toml` in flight; adding a second copy on this branch
would conflict for no benefit, and its `disallowed-methods` entry is for `std::fs::rename`, which this crate
does not call. Extending the lint to `fs::write` was considered and **rejected as written**: `clippy
--all-targets` covers test code, where this crate alone has seven legitimate fixture writes, so the entry
would cost more `#[allow]`s than it would buy signal. A narrower mechanism is worth a separate ticket, not
a hasty entry here.

### Evidence

Guard neutralisation, each break made on its own and restored with `git checkout --` (never a copy â€” a
restored backup's older timestamp leaves `cargo` believing the broken build is current). The fix was
committed **before** any probe, so no probe could take uncommitted work with it. Real output is pasted in
the PR body; the matrix:

| Arm neutralised | Tests red | Tests still green |
|---|---|---|
| `Ok(true)` (link) â†’ `None` | `a_live_alias_â€¦`, `a_dangling_link_â€¦`, `the_junction_fallback_â€¦`, `write_slot_classification_â€¦` | `an_ordinary_file_â€¦` |
| `Ok(false)` (occupied) â†’ `None` | `an_ordinary_file_â€¦`, `write_slot_classification_â€¦` | both link tests |
| `Err(_)` (unknown) â†’ `None` | `write_slot_classification_â€¦` only | everything else |

The link arm reds four tests because four tests exercise **that one arm** â€” it is the arm covering both the
live and the dangling hazard, plus its own pure case and the junction-fixture check. The three arms are
separable and each is proved load-bearing by a set no other arm's break produces.

The live-alias break reproduced the ticket's measurement exactly, `Symlink` staging on this machine:

```
assertion `left == right` failed: the user's unrelated file was overwritten through a Symlink at the destination slot
  left: "---\nid: CPE-9999\ntitle: \"CPE-9999 title\"\ntype: feature\nstatus: Done\npriority: low\ntags: [ready]\n---\n\n## Summary\nbody\n"
 right: "MY NOTES"
```

Restored, recompiled (`Compiling agent-board` printed â€” not a stale-binary green), `cargo test` 27/27 and
`cargo clippy --all-targets -- -D warnings` clean.

### Platform gating â€” nothing skips

`make_dangling_link`'s junction leg needs no privilege, so `a_dangling_link_â€¦` and
`the_junction_fallback_â€¦` run on **every** runner and account, and prove the link arm there. `alias_at`
never skips either: it prefers a symlink and falls back to a hard link, which needs no privilege on NTFS
and stages the same user-visible hazard (a name for a file the user never mentioned, truncated by
`fs::write`) â€” caught by the occupied arm instead of the link arm. Nothing here depends on a skip notice
being seen; under CI they are invisible anyway (CPE-1717).

## Work Log

**Closed 2026-08-13, merged as PR #897 (`44a89fd1`).** Two rounds.

### The bug

`move_card` had no destination guard, and its destructive primitive is `fs::write`, which **follows** the
final path component. A live symlink in the destination column meant the user's unrelated file was
overwritten with ticket frontmatter, the **link survived so the board looked normal**, the source card was
deleted, and the call returned `Ok`. No signal at any point.

Worse than the sites CPE-1710 fixed: those destroy a *link* and fail loudly once guarded. This destroyed
**the contents of a file the user never named** and reported success.

### One stat, not two â€” the structural finding

The rename guards need two probes (`try_exists` for occupancy, `symlink_metadata` for a dangling link)
because `try_exists` **follows** links and answers `Ok(false)` â€” correctly â€” for a dangling one. A *write*
slot needs only `fs::symlink_metadata`, which never follows the final component: `Ok` means taken,
`Err(NotFound)` means provably free, any other `Err` means we could not tell and must not guess.
`try_exists` is not used at this site at all â€” it *structurally cannot see* the case that motivated the
ticket.

Link verdict comes **first** here, the opposite of `fsutil::rename_slot_refusal`, which is clobber-first
only to preserve wording its sites already shipped. This site has no such history, and a live link answers
"taken" to an occupancy check too â€” so link-first is what makes a live link report *that it is a link*.

### The dangling case: refused, and the reasoning is the point

`fs::write` through a dangling link **creates** the target â€” so the board would materialise a file at a
path the user never named, anywhere on disk the link points, while the card looked like it was sitting in
the column. *Nothing about "the far end is empty" makes the far end ours.*

### What the checks added

**A hard link at the destination** is covered by the *occupied* arm â€” and it matters, because on Windows a
hard link needs no privilege and truncating through one destroys the victim exactly as the symlink did.
The reviewer had to stage it deliberately: their runner **had** symlink privilege, so `alias_at` took the
symlink leg and the shipped tests never exercised the hard-link path there.

**A shape the PR never claimed:** a symlink in the destination column pointing at *the source card itself*.
Unguarded, `move_card` wrote the ticket through the link onto its own source and then deleted it â€” **total
ticket loss**, worse than the reported bug. The guard refuses it.

**A scope correction, against the PR's favour:** a junction, a symlink-to-directory, a read-only file and a
directory were **never exploitable on Windows** â€” the OS returns `Access is denied`. So "the junction stages
the identical hazard" overstates it. The genuinely exploitable shapes are symlink-to-**file**, dangling
symlink-to-file, hard link, and plain file. The junction fixture still earns its place because it asserts
`err.contains("is a link")`, so it cannot pass against unguarded code.

### Four claims stronger than the code â€” the sprint's signature failure, fixed at merge

1. **"One stat" is true only of the final component.** The UAT replaced the *column directory* with a
   junction into the user's folder: `move_card` returned `Ok` and materialised a ticket inside it, because
   `symlink_metadata` only ever looked at the leaf. That is the same outcome the dangling-link arm refuses,
   reached one component earlier.
2. **TOCTOU is real and was unmentioned.** Measured: the guard returned `None`, a symlink was planted, the
   victim was overwritten. `std` has no `O_NOFOLLOW` write so it cannot be closed here â€” but "one stat"
   must not read as complete.
3. **`remove_file`'s "orphaned, not destroyed" is false** for a card reached through a junctioned
   *directory*: it deletes the user's real file. Still a move rather than loss â€” the write is ordered
   before the remove precisely so that holds â€” but the reassurance was scoped to the leaf and read as
   general. Also recorded: `let _ =` swallows the error, so an offline far end silently duplicates the
   ticket.
4. **A directory at the slot said "a file already exists."** Now "something" â€” CPE-1687's lesson that
   naming the wrong kind of thing sends the user looking for something that is not there.

### The missing test

The `src == dest` exemption's comment claims a **symlinked** ticket stays re-droppable. Its only test used
an ordinary file, and the reviewer measured that **reversing the documented symlink behaviour left 27/27
green** â€” an unread claim on an untested path, which is exactly what this sprint keeps filing tickets about.

Pinned, and proved *precisely*: a mutation removing the exemption entirely reds **two** tests, which proves
nothing about the specific claim. A mutation reversing **only the link half** â€” ordinary cards still exempt,
linked ones refused â€” reds **exactly one: the new test**. A test that fails for a broad reason is not
evidence for a narrow claim.

### Enumeration and twins

Exactly **two** destructive primitives outside `#[cfg(test)]`, both in `move_card` (`fs::write`,
`fs::remove_file`); no `fs::copy`/`OpenOptions`/`File::create` in the crate; `ui.rs`'s three writes are
inside its test module. Verified independently by both checks.

**Twins clean.** `board_move_impl` writes to `src` â€” the file it just read â€” then renames to a destination
already guarded by CPE-1710. `ticket_mcp` has **zero** `fs::` calls of any kind.

**No `fs::rename` in this crate at all**, which is why CPE-1710's lint could not have caught this even after
its `clippy.toml` reached `sidecar/agent-board`. Two structural blind spots stacked: wrong primitive, and an
uncovered workspace root.

### Nothing skips

The junction fallback needs no privilege and `alias_at` falls back to a hard link, so every leg asserts for
real on every runner â€” `27 passed; 0 ignored`, no skip notice under `--nocapture`, confirmed against the CI
Windows log. That matters because skip notices are **invisible under CI** (CPE-1717).

Verdicts: Reviewer **APPROVE**, UAT **PASS**. All 13 CI checks green.

