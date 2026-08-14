---
id: CPE-1726
title: The FTP, SFTP and WebDAV crates rename onto a remote-supplied path with no destination guard
type: bug
priority: Medium
status: Doing
tags: ready
estimate: M
created: 2026-08-13
closed:
---

## Problem

Found by the PR #899 (CPE-1716) reviewer, 2026-08-13, while independently sweeping every `fs::rename` in
the tree to verify that PR's sibling analysis.

`crates/ftp`, `crates/sftp` and `crates/webdav` each carry an **unguarded `fs::rename` on a path supplied
by the remote client**. They are protocol rename semantics — a client asks the server to rename something —
so they are legitimately different from the app's own save paths, and they were correctly out of scope for
CPE-1716.

But they are **user-reachable destinations**, and this sprint has established what an unguarded rename does:
`fs::rename` does **not** follow the final path component, so a symlink at the destination is **destroyed
and replaced**, silently, while the call returns `Ok`.

## Why this needs thought rather than a reflex fix

Do not simply wrap all three in `fsutil::rename_into_slot`. The question CPE-1716 settled — *"Am I claiming
this name, or editing this file?"* — has a third answer here: **"I am obeying a remote instruction."**

That matters because:

- A protocol server is *supposed* to do what the client asks, within its sandbox. Refusing a rename the
  protocol permits may break a legitimate client with no way for the user to see why.
- But the client is **not** the person whose files are at the destination. A user running the SFTP server
  to share a folder did not agree to have their symlinks replaced by whoever connects.
- These crates sit **outside `cpe-server`**, so `fsutil`'s helpers are a cross-crate dependency, not a local
  call. CPE-1719 faced the same boundary in `sidecar/agent-board` and reimplemented rather than depended —
  check whether that precedent applies or whether these crates already depend on `cpe-server`.

**Decide per crate and record the reasoning.** They may not get the same answer.

## Scope

The `fs::rename` sites in `crates/ftp`, `crates/sftp` and `crates/webdav`. Check for the sibling primitives
at the same time — `fs::write` **follows** a link and writes *through* it, which is a different failure with
a different fix (see CPE-1719), and `remove_file` on a link removes the link.

Note **CPE-1710's `clippy.toml` bans bare `std::fs::rename` in all 17 workspace roots**, so each of these
sites already carries an `#[allow(clippy::disallowed_methods)]` with a stated reason. **Read those reasons
first** — they were written during CPE-1710's sweep and classified as "a protocol server's own rename
semantics". This ticket is the follow-up that asks whether that classification is sufficient.

## Acceptance criteria

- [ ] For each of the three crates: either the destination is guarded, or the reason it is safe to obey the
      client unguarded is **measured and written at the site**. "It is a protocol server" is a category, not
      a measurement.
- [ ] A test per crate proves what happens when a **symlink** sits at the rename destination — live and
      dangling. **Assert on the slot and on the victim's bytes, never on the returned `Result`**; every bug
      in this family returned `Ok` while destroying something.
- [ ] If any crate is left deliberately unguarded, its `#[allow]` reason says so explicitly and names this
      ticket, so the next sweep does not re-open it from scratch.
- [ ] Check the other destructive primitives in these crates (`fs::write`, `remove_file`, `File::create`,
      `OpenOptions`) rather than fixing only `rename`. CPE-1719 was missed precisely because the primitive
      differed from the one the previous ticket swept for.
- [ ] Platform-gate correctly. Creating a symlink on Windows needs Developer Mode or elevation; a junction
      (directory-only) or hard link (`is_symlink=false`) is **not** a substitute for a live file symlink —
      that was measured on CPE-1716. If a leg cannot stage on an unprivileged runner, use the pure-classifier
      split CPE-1716 used so something is still covered everywhere.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md` (amended 2026-08-13 — read the current text).

## Notes

Filed by the Foreman from the PR #899 review, 2026-08-13. The reviewer flagged these as out of scope for
that PR and worth knowing about — the right call on both counts.

Related: **CPE-1710** (the pairing and the clippy ban that made these sites visible), **CPE-1716** (the
resolve-vs-claim distinction and the Windows staging constraints), **CPE-1719** (the sidecar precedent for
reimplementing rather than depending across a crate boundary, and the `fs::write`-follows-a-link failure),
**CPE-1461** (the traversal guard these crates already carry).
