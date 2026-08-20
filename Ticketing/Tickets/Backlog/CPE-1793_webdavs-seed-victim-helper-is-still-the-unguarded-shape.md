---
id: CPE-1793
title: webdav's cpe_1730_seed_victim_outside is still the unguarded shape — it leaks only on the panic path
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

`crates/webdav/src/lib.rs:1975` — `cpe_1730_seed_victim_outside` still returns a bare `PathBuf` with a
trailing `let _ = std::fs::remove_dir_all(&victim_dir)` at its two call sites (`:2224`, `:2309`).

That is exactly the second-level-helper, panic-unsafe pattern CPE-1693 was created to eliminate and
which CPE-1782 has just closed in `crates/sftp` and `crates/ftp`. CPE-1693 converted webdav's
*spawners* and missed this helper.

**It leaks only on the panic path**, which is why no measurement has ever caught it: on a green run the
trailing `remove_dir_all` does its job, and every leak census this crew has run counted directories
after a *passing* suite. The exposure is real but invisible to the method used so far.

## What to do

- Convert it to the shape the other seven crates now use: return `(ScratchDir, PathBuf, ...)` and rebind
  both halves at each call site — never `let _ = ...`.
- Check drop ordering. In the CPE-1730 tests the victim directory is deliberately outside the server
  root and reached through a junction, so the victim guard must drop **before** the root that contains
  the junction pointing at it, or the cleanup walks through a reparse point. CPE-1782's sftp/ftp
  conversion got this right by declaring the guards in the correct order; match it.
- **Red-proof on the panic path specifically** — a green run proves nothing here, for the same reason
  nobody has noticed it. Arm the guard, panic mid-assertion inside `catch_unwind`, assert the panic
  actually fired *before* asserting the directory is gone (so the test cannot pass vacuously), per the
  proof shape in `crates/sftp/src/lib.rs:1064`.

## Two small companions worth folding in

Both raised as non-blocking on PR #941 and cheap to do in the same pass:

- `crates/net/src/lib.rs:57` — `scratch()`'s doc comment still says the directory is "cleaned up by the
  OS temp reaper". Since CPE-1693 it returns a guard that removes the directory itself. A comment that
  describes the old behaviour is how the next person reasons wrongly.
- CPE-1782's AC #5 asks that any site genuinely unable to own its guard be documented at the call site
  with the reason. Two remain undocumented: `crates/net/examples/security_demo.rs:22` and
  `crates/net/src/bin/cpe-server-ref.rs:27`. Both are deliberate — a server/demo data directory should
  not self-delete — so a one-line note at each closes it honestly.

## Notes

Filed by the Foreman from PR #941's review, 2026-08-19. The reviewer found it while running an
independent census that filtered on neither helper name nor return type — the same technique that
uncovered the 69th and 70th helpers on CPE-1693.

Related: **CPE-1782** (the sftp/ftp/net conversion this completes), **CPE-1693** (the original sweep),
**CPE-1730** (the tests this helper serves).
