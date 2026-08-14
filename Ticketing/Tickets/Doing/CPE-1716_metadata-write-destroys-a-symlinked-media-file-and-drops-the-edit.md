---
id: CPE-1716
title: Editing metadata on a symlinked media file destroys the link, drops the edit, and reports success
type: bug
priority: High
status: Doing
tags: ready
estimate: M
created: 2026-08-13
closed:
---

## Problem

Found by the PR #895 (CPE-1710) UAT, 2026-08-13, while testing that PR's claim to have enumerated every
`fs::rename` in the tree. **This site was classified out of class** as *"the destination is a file we own
and are deliberately replacing."* It is not — it is **the user's own media file path**, straight from the
explorer.

`src-tauri/src/lib.rs:3546` (`metadata_write`) writes a temp file and `fs::rename`s it over the target.
`fs::rename` does **not** follow the final path component, and this site never calls `clobber_refusal` at
all — so unlike the six sites CPE-1710 fixes, **a live symlink is at risk here, not only a dangling one.**

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
2. The **real media file is never edited** — it still has the old metadata.
3. The UI reports **success** and echoes back the edited field, so the user has positive confirmation of a
   thing that did not happen.

The six sites CPE-1710 fixes at least fail loudly once guarded. This one **lies**. A user with a music
library organised by symlinks — a completely ordinary arrangement — edits a track's title, sees it applied,
and has silently forked their library.

## Root cause of the miss, worth recording

The PR's framing was *"only a dangling link is at risk, because `clobber_refusal` catches live ones."* That
is true **only where `clobber_refusal` is actually called.** At a site with no guard at all, both live and
dangling links are destroyed. The classification inherited an assumption from the guarded sites and applied
it to an unguarded one.

## Scope

`src-tauri/src/lib.rs`'s `metadata_write` and its temp-file-then-rename pattern. Check the sibling metadata
paths at the same time — if one write path has this shape, its neighbours probably do.

## Acceptance criteria

- [ ] Writing metadata to a **symlinked** file either edits the file the link points at, or refuses and
      says why. Destroying the link is not acceptable, and neither is reporting success for an edit that
      did not land.
- [ ] Decide and record which of those two it should be. Following the link is the behaviour a user
      expects from an editor; refusing is safer but will surprise anyone with a symlinked library. If you
      follow the link, say what happens when the link is **dangling**.
- [ ] The success report must reflect what actually happened. The current failure is not only the lost
      link — it is that the UI confirmed an edit that never reached the file.
- [ ] A test asserts on **the file the user opens** and on **the slot still being a symlink** — never on
      the returned `Result`, which was `ok: true` throughout this bug.
- [ ] Check the other atomic tmp→final rename sites that CPE-1710 classified as "a file we own": the
      journals, the index, the vault, `known_hosts`. **Verify ownership rather than assuming it** — if any
      destination is user-reachable, it has this bug too.
- [ ] Platform-gate correctly. Creating a symlink on Windows needs Developer Mode or elevation; a test that
      silently no-ops on an unprivileged runner proves nothing. `fsutil::make_dangling_link`'s junction
      fallback is the pattern to copy — and note it is currently `#[cfg(test)] pub(crate)` inside
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

- **`vault_crypto::promote` — same shape as this ticket, and live.** `out_dir` is the user's unlock
  destination, not "a file we own". `promote` probes occupancy with `read_dir(out_dir)`, which **follows**
  the link: on a dangling link it returns `NotFound`, falls through, and `fs::rename(staging, out_dir)`
  destroys it. Fix it here, with this one.
- **`vault_manager` ×2 — right conclusion, wrong stated reason.** The destination is the user's chosen
  `.cpevault` path. The code *deliberately* replaces a symlink there, with a rationale and a
  VAULT-SECURITY.md §5 reference. Leaving it alone is correct; "a file we own" is not why. Fix the
  justification, not the code.
- **`crates/server/src/provider.rs:156` (`LocalProvider::rename`)** — unguarded, and in neither of PR
  #895's lists. Currently `#![allow(dead_code)]` pending CPE-685, so **not user-reachable today** — but it
  must get the pairing before any command routes through providers.

Verified sound, for the record, so nobody re-audits them: `audit_journal`, `checkpoint_store`,
`metrics_journal`, `replay_baseline` (all `file.with_extension(".tmp")` → app-data journal); `known_hosts`
(documented contract that callers never point it at the user's real `~/.ssh/known_hosts`); `index`,
`semantic_index`, `vector_index` (destinations from `index_service::volume_path` /
`content_index::index_path`, both app-managed).

## Work Log — 2026-08-13

**Decision: RESOLVE the link.** `metadata_write` now edits the file the link points at, and the link stays
a link. A **dangling** link is **refused** with a message naming it — there is nothing to edit, and the two
alternatives (invent the target, or rename over the link) are respectively a surprise and this bug. The
reasoning is recorded at `crates/server/src/fsutil.rs` on `replace_file_contents`, including why this is
deliberately the *opposite* of `vault_manager`'s settled replace-the-link decision: a vault write **claims a
name the user typed**, this **edits a file the user already has open**. "Am I claiming this name, or editing
this file?" is the question that picks the helper.

**Shape of the fix.** New `cpe_server::fsutil::replace_file_contents(path, bytes)` — the counterpart to
`rename_into_slot`. `rename_into_slot` was the wrong tool here twice over: its occupancy half would refuse
the very file the call exists to rewrite, and its link half would refuse a symlinked path that must be
resolved *past*. The decision is split into a pure `classify_write_target` for a concrete reason, not for
style: a live **file** symlink cannot be staged on an unprivileged Windows runner at all (junctions are
directory-only), so with the decision inline the live-link arm would be covered on Unix and nowhere else.
The staging temp is opened with `create_new` (`O_CREAT|O_EXCL`), which does not follow a link at the final
component, so the temp cannot be written through a pre-placed one either.

`metadata_write`'s body moved to `metadata_write_impl` so the save is testable without a Tauri runtime —
the bug returned `Ok` throughout, so only a test that asserts on the *file* can catch it.

**Sibling write paths — checked, scope stated.** Swept every `fs::write`/`fs::rename` in `src-tauri/src` and
`crates/server/src` for a destination that is a user path. `metadata_write` was the **only** user-file writer
in the metadata/media family using temp→rename onto the user's own path. Its neighbours — `write_file_text`
(content editor), `macro_convert_in_place`, `batch_execute`'s in-place overwrite, `forge_resolve_file` — all
use `fs::write`, which **follows** a link and writes through it, so none of them has this shape. They carry
the inverse trade-off instead (the edit lands and the link survives, but the write is not atomic); that is a
different decision and is **not** changed here. `replace_file_contents` now exists if it is ever wanted.

**The other sites in this ticket were already closed by CPE-1710 (#895) before this branch started:**

- `vault_crypto::promote` — fixed on `main`: `symlink_slot_refusal` runs before the `read_dir`, with two
  tests. Its own comment records the corrected measurement (a *dangling* link is refused by the OS first
  with `ENOTDIR`/os error 267; the real loss was a **live** directory link over an empty target, where
  `read_dir` follows, `remove_dir` deletes the link, and the rename succeeds `Ok(())`).
- `vault_manager` ×2 — justification already corrected on `main`; the code is deliberately unchanged.
- `provider.rs` `LocalProvider::rename` — still unguarded, still unreachable (`#![allow(dead_code)]` pending
  CPE-685). The KNOWN GAP note is at the site. **Noted, not fixed**, per this ticket's scope.
- The journals / index / `known_hosts` destinations were spot-verified (app-data dirs; `checkpoint_store`
  hashes the user's root rather than joining it) — no user-reachable destination among them.

**Guard neutralisation** (Evidence Rules §1) — three arms broken one at a time, real output in the PR body.
Every leg ran for real on this machine (Developer Mode is on; no skip notice printed under `--nocapture`).

**Also related, different primitive:** `sidecar/agent-board::move_card` (`board.rs:395`) has no
destination guard at all — `fs::write(&dest, ..)` then `remove_file(&src)`. It is the twin of the
`board_move_impl` PR #895 fixes, and the "both board implementations change in lockstep" rule says it
should move with it. `fs::write` **follows** a link and writes *through* it, so the failure differs from a
rename, but it is the same slot and the same user-visible operation.
