---
id: CPE-1716
title: Editing metadata on a symlinked media file destroys the link, drops the edit, and reports success
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-13
closed: 2026-08-14
---

## Problem

Found by the PR #895 (CPE-1710) UAT, 2026-08-13, while testing that PR's claim to have enumerated every
`fs::rename` in the tree. **This site was classified out of class** as *"the destination is a file we own
and are deliberately replacing."* It is not â€” it is **the user's own media file path**, straight from the
explorer.

`src-tauri/src/lib.rs:3546` (`metadata_write`) writes a temp file and `fs::rename`s it over the target.
`fs::rename` does **not** follow the final path component, and this site never calls `clobber_refusal` at
all â€” so unlike the six sites CPE-1710 fixes, **a live symlink is at risk here, not only a dangling one.**

Measured through the real command:

```
[UAT] before: link is_symlink=true
[UAT] metadata_write -> ok=true
[UAT] returned fields: [MetaField { group: "wav", key: "Title", value: "EDITED", editable: true }]
[UAT] AFTER: the user's symlink still a link = false
[UAT] AFTER: the REAL file got the edit? false  (len 39 -> 39)
```

## Why this is worse than the bugs CPE-1710 fixed

Three things go wrong at once and **all of them are silent**:

1. The user's **symlink is destroyed**, replaced by a regular file.
2. The **real media file is never edited** â€” it still has the old metadata.
3. The UI reports **success** and echoes back the edited field, so the user has positive confirmation of a
   thing that did not happen.

The six sites CPE-1710 fixes at least fail loudly once guarded. This one **lies**. A user with a music
library organised by symlinks â€” a completely ordinary arrangement â€” edits a track's title, sees it applied,
and has silently forked their library.

## Root cause of the miss, worth recording

The PR's framing was *"only a dangling link is at risk, because `clobber_refusal` catches live ones."* That
is true **only where `clobber_refusal` is actually called.** At a site with no guard at all, both live and
dangling links are destroyed. The classification inherited an assumption from the guarded sites and applied
it to an unguarded one.

## Scope

`src-tauri/src/lib.rs`'s `metadata_write` and its temp-file-then-rename pattern. Check the sibling metadata
paths at the same time â€” if one write path has this shape, its neighbours probably do.

## Acceptance criteria

- [ ] Writing metadata to a **symlinked** file either edits the file the link points at, or refuses and
      says why. Destroying the link is not acceptable, and neither is reporting success for an edit that
      did not land.
- [ ] Decide and record which of those two it should be. Following the link is the behaviour a user
      expects from an editor; refusing is safer but will surprise anyone with a symlinked library. If you
      follow the link, say what happens when the link is **dangling**.
- [ ] The success report must reflect what actually happened. The current failure is not only the lost
      link â€” it is that the UI confirmed an edit that never reached the file.
- [ ] A test asserts on **the file the user opens** and on **the slot still being a symlink** â€” never on
      the returned `Result`, which was `ok: true` throughout this bug.
- [ ] Check the other atomic tmpâ†’final rename sites that CPE-1710 classified as "a file we own": the
      journals, the index, the vault, `known_hosts`. **Verify ownership rather than assuming it** â€” if any
      destination is user-reachable, it has this bug too.
- [ ] Platform-gate correctly. Creating a symlink on Windows needs Developer Mode or elevation; a test that
      silently no-ops on an unprivileged runner proves nothing. `fsutil::make_dangling_link`'s junction
      fallback is the pattern to copy â€” and note it is currently `#[cfg(test)] pub(crate)` inside
      `cpe-server`, so it is **unreachable from `src-tauri`**; that visibility is why three CPE-1710 sites
      shipped untested. Fix the visibility or provide an equivalent.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #895 UAT, 2026-08-13. Filed separately rather than folded into CPE-1710
because it is a **user-data bug with a false success report**, not a hardening gap, and it deserves its own
priority rather than riding along with a refusal-guard ticket.

Related: **CPE-1710** (which enumerated the renames and misclassified this one), **CPE-1715** (the
name-picking probes with a related dangling-link blind spot), **CPE-1705** (the stat-collapse family this
belongs to).

## Three more sites in the same class (PR #895 review, 2026-08-13)

The reviewer audited all twelve out-of-class entries independently. **Eight are sound**; four are
mis-stated, and one of those is live:

- **`vault_crypto::promote` â€” same shape as this ticket, and live.** `out_dir` is the user's unlock
  destination, not "a file we own". `promote` probes occupancy with `read_dir(out_dir)`, which **follows**
  the link: on a dangling link it returns `NotFound`, falls through, and `fs::rename(staging, out_dir)`
  destroys it. Fix it here, with this one.
- **`vault_manager` Ã—2 â€” right conclusion, wrong stated reason.** The destination is the user's chosen
  `.cpevault` path. The code *deliberately* replaces a symlink there, with a rationale and a
  VAULT-SECURITY.md Â§5 reference. Leaving it alone is correct; "a file we own" is not why. Fix the
  justification, not the code.
- **`crates/server/src/provider.rs:156` (`LocalProvider::rename`)** â€” unguarded, and in neither of PR
  #895's lists. Currently `#![allow(dead_code)]` pending CPE-685, so **not user-reachable today** â€” but it
  must get the pairing before any command routes through providers.

Verified sound, for the record, so nobody re-audits them: `audit_journal`, `checkpoint_store`,
`metrics_journal`, `replay_baseline` (all `file.with_extension(".tmp")` â†’ app-data journal); `known_hosts`
(documented contract that callers never point it at the user's real `~/.ssh/known_hosts`); `index`,
`semantic_index`, `vector_index` (destinations from `index_service::volume_path` /
`content_index::index_path`, both app-managed).

## Work Log â€” 2026-08-13

**Decision: RESOLVE the link.** `metadata_write` now edits the file the link points at, and the link stays
a link. A **dangling** link is **refused** with a message naming it â€” there is nothing to edit, and the two
alternatives (invent the target, or rename over the link) are respectively a surprise and this bug. The
reasoning is recorded at `crates/server/src/fsutil.rs` on `replace_file_contents`, including why this is
deliberately the *opposite* of `vault_manager`'s settled replace-the-link decision: a vault write **claims a
name the user typed**, this **edits a file the user already has open**. "Am I claiming this name, or editing
this file?" is the question that picks the helper.

**Shape of the fix.** New `cpe_server::fsutil::replace_file_contents(path, bytes)` â€” the counterpart to
`rename_into_slot`. `rename_into_slot` was the wrong tool here twice over: its occupancy half would refuse
the very file the call exists to rewrite, and its link half would refuse a symlinked path that must be
resolved *past*. The decision is split into a pure `classify_write_target` for a concrete reason, not for
style: a live **file** symlink cannot be staged on an unprivileged Windows runner at all (junctions are
directory-only), so with the decision inline the live-link arm would be covered on Unix and nowhere else.
The staging temp is opened with `create_new` (`O_CREAT|O_EXCL`), which does not follow a link at the final
component, so the temp cannot be written through a pre-placed one either.

`metadata_write`'s body moved to `metadata_write_impl` so the save is testable without a Tauri runtime â€”
the bug returned `Ok` throughout, so only a test that asserts on the *file* can catch it.

**Sibling write paths â€” checked, scope stated.** Swept every `fs::write`/`fs::rename` in `src-tauri/src` and
`crates/server/src` for a destination that is a user path. `metadata_write` was the **only** user-file writer
in the metadata/media family using tempâ†’rename onto the user's own path. Its neighbours â€” `write_file_text`
(content editor), `macro_convert_in_place`, `batch_execute`'s in-place overwrite, `forge_resolve_file` â€” all
use `fs::write`, which **follows** a link and writes through it, so none of them has this shape. They carry
the inverse trade-off instead (the edit lands and the link survives, but the write is not atomic); that is a
different decision and is **not** changed here. `replace_file_contents` now exists if it is ever wanted.

**The other sites in this ticket were already closed by CPE-1710 (#895) before this branch started:**

- `vault_crypto::promote` â€” fixed on `main`: `symlink_slot_refusal` runs before the `read_dir`, with two
  tests. Its own comment records the corrected measurement (a *dangling* link is refused by the OS first
  with `ENOTDIR`/os error 267; the real loss was a **live** directory link over an empty target, where
  `read_dir` follows, `remove_dir` deletes the link, and the rename succeeds `Ok(())`).
- `vault_manager` Ã—2 â€” justification already corrected on `main`; the code is deliberately unchanged.
- `provider.rs` `LocalProvider::rename` â€” still unguarded, still unreachable (`#![allow(dead_code)]` pending
  CPE-685). The KNOWN GAP note is at the site. **Noted, not fixed**, per this ticket's scope.
- The journals / index / `known_hosts` destinations were spot-verified (app-data dirs; `checkpoint_store`
  hashes the user's root rather than joining it) â€” no user-reachable destination among them.

**Guard neutralisation** (Evidence Rules Â§1) â€” three arms broken one at a time, real output in the PR body.
Every leg ran for real on this machine (Developer Mode is on; no skip notice printed under `--nocapture`).

## Work Log â€” round 2 (PR #899 UAT + Reviewer, 2026-08-13)

UAT **PASS** (fourteen adversarial link shapes, atomicity measured under a forced mid-save rename
failure) and Reviewer **APPROVE**. Six corrections landed, one of them user-facing:

- **F3, user-facing.** The refusal was **unreachable from its only caller**: `fs::read` follows a link and
  failed first, so a dangling link produced `The system cannot find the file specified. (os error 2)` â€”
  no path, no mention of a link â€” while the shipped user docs promised a message naming the link.
  `metadata_write_impl` now calls `resolve_write_target` **before** the read and reads/writes the resolved
  path, so the good message actually arrives (and the bytes read and the bytes written provably concern
  the same file). New test `cpe_1716_metadata_write_refuses_a_dangling_link_with_a_message_that_names_it`
  runs on **every** runner via the junction fallback.
- **F1.** The claim that a skip notice is invisible under CI was **wrong**, proved three times (UAT
  controlled experiment, Reviewer's independent one, and my own probe below). Under a plain `cargo test`
  libtest swallows `println!`/`eprintln!` for a *passing* test but **not** `writeln!(std::io::stderr())`.
  Every skip notice here already used the right emitter; only the prose was wrong. Fixed in
  `fsutil.rs`, `lib.rs` and the PR body.
- **F2.** Because the notices are visible, their **absence is evidence**: CI run `31772062682` shows both
  live-link tests passing on `windows-latest` with no `[CPE-1716] SKIPPED` line, so the live-link route
  ran for real on all three legs. The `classify_write_target` split buys coverage for an unprivileged
  *contributor* machine, not for CI â€” stated that way now.
- **F4.** "`create_new` cannot be tested" was too strong. Pinned at the primitive with an `fs::write`
  contrast; Probe E below shows `create(true).truncate(true)` following a dangling **junction** and
  creating the target.
- **Reviewer doc corrections.** The `rename_into_slot` rationale now matches measurement (on a *live*
  link the **occupancy** half refuses first and the link half is never reached; and after resolving, the
  occupancy half still refuses the resolved target â€” so this is a necessary primitive, not duplication).
  The `NotFound` arm no longer claims proof of absence: Rust folds `ERROR_BAD_NETPATH`/`BAD_NET_NAME`/
  `INVALID_DRIVE` into it, so a disconnected UNC reaches it and only `create_new` stops the write.
- **Docs over-claim Ã—2, both fixed.** The refusal message is now real (F3), and "a crash or a power cut
  can never leave you with a half-written file" is scoped to **interrupted saves**: the parent directory
  is not fsynced, and `vault_manager::sync_parent_dir` shows why a uniform power-loss claim is not
  available (Unix-only, and explicitly *narrowed not closed* on Windows). Recorded at the site.
- **Minors.** `display_path` strips the `\\?\` verbatim prefix from user-facing errors; the staging temp
  landing beside the **resolved** target (required for rename atomicity) is now documented as the
  behaviour change it is.
- **Filed CPE-1725** â€” `write_file_text` returns `Ok` and *creates* the target through a dangling link
  where `metadata_write` now refuses. Not destruction; the two save paths simply disagree, and the other
  three `fs::write` siblings share the shape, so it is a four-command decision of its own.

Probes D and E, one at a time, each reddening a **distinct** test (real output in the PR body).
`crates/ftp` / `crates/sftp` / `crates/webdav` renames: out of scope, filed by the Foreman as CPE-1726,
untouched.

**Also related, different primitive:** `sidecar/agent-board::move_card` (`board.rs:395`) has no
destination guard at all â€” `fs::write(&dest, ..)` then `remove_file(&src)`. It is the twin of the
`board_move_impl` PR #895 fixes, and the "both board implementations change in lockstep" rule says it
should move with it. `fs::write` **follows** a link and writes *through* it, so the failure differs from a
rename, but it is the same slot and the same user-visible operation.

## Work Log

**Closed 2026-08-14, merged as PR #899 (`9339670c`).** Three rounds.

### The bug

`metadata_write` staged a temp file and `fs::rename`d it over the target. `fs::rename` does **not** follow
the final component, and the site had no guard â€” so for a symlinked track **three things went wrong at
once, all silent**: the link was destroyed and replaced by a regular file, the real media file was never
edited, and the UI **reported success and echoed the edited field back**. A user with a symlink-organised
library edits a title, sees it applied, and has silently forked their library.

The sites CPE-1710 fixed at least fail loudly once guarded. This one **lied**.

### The decision, and the question that decides the next one

**Resolve the link** â€” edit the file it points at, keep the link â€” and **refuse a dangling link**, naming it.

Deliberately the **opposite** of `vault_manager`'s settled replace-the-link behaviour, with the
discriminating question written at the call site:

> **"Am I claiming this name, or editing this file?"**

Claiming a name the user typed â†’ replace what is there. Editing a file the user already has open â†’ follow
the link to it. Both checks judged that distinction principled rather than a rationalisation, and it maps
onto an observable: does the caller hold bytes it read from that path?

Resolving is right because in a symlink-organised library **every entry is a link**, so refusing would make
the Metadata Studio useless for exactly the people most likely to open it â€” and resolving cannot write
somewhere unexpected, because the resolved path is wherever the user's own link points.

### `replace_file_contents` is necessary, not duplication â€” measured

`rename_into_slot` was wrong here, and the reviewer established *why* rather than accepting it: even after
resolving, its **occupancy** half still refuses the resolved target (`1d -> Some("real.wav already
exists")`). One correction: "wrong twice over" is imprecise â€” for a **live** link `clobber_refusal` runs
first and `try_exists` follows the link, so the link half is never reached; it fires only on a dangling
symlinked path.

### Fourteen adversarial shapes, nothing damaged

The UAT drove: outside the library, a **directory**, 2-hop and **41-hop** chains, a **loop**, a relative
target, a link to a dangling link, a read-only far end, a dead UNC path, `NUL`/`CON`, `\\.\NUL`, and a
**junction to a file**. Every one: link still a link, `strays = []`, nothing of the user's damaged.

Atomicity measured for real by holding the victim open `FILE_SHARE_NONE` to force the rename to fail
mid-save â€” **victim intact and whole, not truncated**.

### Round 2: the good refusal was unreachable

The carefully-worded dangling-link message could not arrive. `fs::read` follows the link and fails **first**,
so the user got a bare `The system cannot find the file specified. (os error 2)` â€” no path, no mention of a
link â€” while the **shipped user docs** promised *"the save is refused and says so."* Reordered so the
message arrives; the "creates nothing at the would-be target" property survived.

Four claims were corrected alongside it, all cases of prose outrunning code:

1. *"Per CPE-1717 that notice is invisible under CI"* â€” **false**, in two code sites and the PR body. The
   capture is inside the `print!`/`eprint!` macros, so `writeln!(stderr)` goes around it.
2. Because the notices are visible, **their absence is evidence** â€” both live-link legs are recorded
   running on `windows-latest` with no skip line, so the disclosed gap bites a contributor machine, not CI.
3. *"Provably holds nothing"* is overstated on Windows: `NotFound` also covers `ERROR_BAD_NETPATH` /
   `ERROR_BAD_NET_NAME` / `ERROR_INVALID_DRIVE`. Measured â€” a dead UNC classifies as free and `create_new`
   is the **only** thing that stops the write.
4. The docs' *"the save is atomic"* is now scoped to interrupted saves, excluding power cuts: the parent
   directory is never fsynced, and `vault_manager::sync_parent_dir` is Unix-only.

### Round 3: a test named after a guard, passing with the guard removed

The round-2 test added to close the "`create_new` is untestable" finding **built its own `OpenOptions`
closure** and never called `replace_file_contents`. Swapping `create_new(true)` for
`create(true).truncate(true)` at the production call site left the full suite green â€” **including that
test**. It pinned `std::fs`'s semantics, not this crate's use of them. **Evidence Rule 1, inside the change
made to close a finding that was itself about testability.** And the comment beside it asserted the
neutralisation redded it, which was false.

Fixed by extracting `stage_exclusive` â€” one line, one caller, no behaviour change, whose whole purpose is
to be reachable from a test. The same neutralisation now reds, and the panic is better than an assertion:

```
create_new must refuse a link already at the name:
  File { path: "...\staging-dangling-target-that-does-not-exist" }
```

The open **succeeded and followed the dangling link**, conjuring a file nobody named â€” the hazard itself,
in the failure message.

The UAT's judgement, which drove the choice: round 1's honest *"not a tested guard and is not claimed as
one"* was better than the false claim that replaced it. **An untested guard you admit to beats a
tested-looking one you don't.**

### Also settled

The **sibling sweep** was verified independently: `metadata_write` was the only user-file tempâ†’rename;
every other such site targets app-private data with a recorded justification. Leaving the four `fs::write`
siblings alone is right â€” swapping them would trade an unreported non-atomicity for a behaviour change
across four commands, in a PR about data loss. `vault_crypto::promote` and the `vault_manager`
justifications were **already fixed by CPE-1710**, and the worker checked rather than redoing them.

The coverage caveat was verified from scratch: a junction-to-a-file reports `is_symlink=true` but fails
`canonicalize` with `NotADirectory`, and a hard link is `is_symlink=false` â€” so **no privilege-free
construction exists** and the skip is not self-inflicted. CI's `windows-latest` holds the privilege anyway.

Verdicts: Reviewer **APPROVE**, UAT **PASS** (round 3). All 12 CI checks green â€” the long pole was
`Server crates (windows-latest)` at 1h0m1s, an hour behind the Backend job that finishes in six minutes.

Filed, not fixed: **CPE-1725** â€” `write_file_text` through a dangling link returns `Ok` and **creates** the
target, where this refuses. Two save paths answering the same question oppositely.

