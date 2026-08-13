---
id: CPE-1716
title: Editing metadata on a symlinked media file destroys the link, drops the edit, and reports success
type: bug
priority: High
status: Backlog
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
