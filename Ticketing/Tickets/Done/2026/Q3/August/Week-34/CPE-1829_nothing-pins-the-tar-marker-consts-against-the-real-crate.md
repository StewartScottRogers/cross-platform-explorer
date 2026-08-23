---
id: CPE-1829
title: nothing pins the tar link-marker constants against the real crate, so a tar bump silently reverts CPE-1813
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-23
---

## Problem

CPE-1813 makes TAR deliver ZIP's no-link-support refusal by recovering the OS error out of `tar`'s
rewrapped error text, anchored to two literal marker strings — `" when symlinking "` and
`" when hard linking "` (`TAR_SYMLINK_MARKER` / `TAR_HARDLINK_MARKER` in `crates/server/src/archive.rs`).

**Every test pins those constants against a test double built from the same constants.** So a future
`tar` bump that reworded its wrap would make `recover_link_syscall_error` return `None` everywhere:
TAR silently reverts to the pre-CPE-1813 abort, the whole suite stays green, and nobody is told. A
feature that disappears without a red test is exactly the failure this ticket family exists to prevent.

## What to do

The CPE-1813 Reviewer demonstrated the test while reviewing — roughly ten lines, no OS privilege
needed, works on every platform:

- Pre-occupy the destination so a real `fs::hard_link` genuinely fails.
- Reuse the existing `craft_tar_with_hard_link` fixture and drive a real `Entry::unpack_in`.
- Assert `recover_link_syscall_error(&err, TAR_HARDLINK_MARKER).is_some()`.

It would be the only assertion in the file that touches the **real dependency** rather than a double.
The Reviewer's own probe against `tar-0.4.46` confirms the shape it must pin: the real leaf renders
`"Cannot create a file when that file already exists. (os error 183) when hard linking … to …"`, and
`recover_link_syscall_error` returns `Some((Some(183), AlreadyExists))`.

Do the symlink marker too if it can be triggered without `SeCreateSymbolicLinkPrivilege`; if it cannot,
say so rather than writing a leg that silently skips.

## Also — a doc note to land with it

Two leaf-shaped wraps neither the author nor the Reviewer had enumerated: `tar entry.rs:514` and `:522`
push the raw 512-byte, fully attacker-controlled tar header into a leaf error's text
(`other(&format!("hard link listed for {} …", String::from_utf8_lossy(header.as_bytes())))`), so they
**can** carry either marker.

They are harmless today for two independent reasons: the kind is hardcoded `ErrorKind::Other`, which
`link_creation_is_categorical` rejects; and `tar_entry_refusal` refuses an empty or missing link target
before `unpack_in` is reached. Record that in `recover_link_syscall_error`'s doc, **and say out loud that
the safety there is a kind coincidence, not a structural property** — so the next person does not
re-derive it and does not assume it is guaranteed.

## Acceptance criteria

- [ ] A test drives the real `tar` crate to a genuine link-creation failure and asserts the marker
      recovery succeeds, so a reworded upstream wrap goes red instead of silent.
- [ ] Red-proof it: change one marker const, observe the new test red, revert, record the line.
- [ ] The `entry.rs:514`/`:522` note lands in `recover_link_syscall_error`'s doc with the kind-coincidence
      caveat stated explicitly.

## Notes

Filed from the CPE-1813 round-3 review, which classified both items as follow-ups rather than merge
blockers and verified the current enumeration is complete against `tar-0.4.46`'s source.

The Reviewer also recorded the structural alternative for whenever this area is next opened: the
`" while canonicalizing "` exclusion is a **blocklist**, and blocklists go stale. The stronger rule is to
require the prefix to be *exactly* what an error at that level renders — `prefix ==
from_raw_os_error(code).to_string()` rather than `starts_with`, and for the kind-only arm require a bare
OS message rather than merely the absence of a known phrase. Not worth reopening CPE-1813 for; worth
doing if that code is touched again.

## Work Log

- 2026-08-23 — Implemented on branch `cpe-1829-pin-tar-link-markers`, in `crates/server/src/archive.rs`.

  **Two new tests drive the real `tar-0.4.46` crate's `Entry::unpack_in` to a genuine link-creation
  failure**, instead of `wrap_like_tar_link_syscall_failure`'s hand-built `FakeTarError` double every
  other test in the file uses:

  - `cpe1829_recover_link_syscall_error_pins_against_the_real_tar_crate_hardlink` — runs on every
    platform. Pre-occupies the entry's destination name (`dest/hard`) with an ordinary file before
    calling the real `entry.unpack_in(&dest)` on a hard-link entry built with the existing
    `craft_tar_with_hard_link` fixture, so `fs::hard_link` genuinely fails `AlreadyExists`. Asserts
    `err.raw_os_error().is_none()` (precondition: the real crate did discard the code, or the test isn't
    exercising the hazard) and `recover_link_syscall_error(&err, TAR_HARDLINK_MARKER).is_some()` with
    `kind() == AlreadyExists`. Chosen over the symlink marker for the all-platform leg because `tar`'s
    hard-link arm (`entry.rs:546-556`) has no remove-and-retry on an occupied name, unlike its symlink
    arm — the failure mode is deterministic everywhere.
  - `cpe1829_recover_link_syscall_error_pins_against_the_real_tar_crate_symlink` — **`#[cfg(unix)]` only,
    by assumption, not a runtime skip.** Windows symlink creation needs `SeCreateSymbolicLinkPrivilege`
    (admin or Developer Mode), which the CI runner cannot be assumed to have, so this leg does not run on
    Windows at all; the hardlink test above already pins the real crate on every platform including
    Windows, just not this marker. On Unix (no privilege needed) it pre-occupies the entry's destination
    name with a **directory** rather than a file — a file would trigger `tar`'s own overwrite contract
    (`remove_file` + retry, `entry.rs:561-568`) and silently succeed instead of failing; a directory can
    never be removed by `remove_file`, so the real symlink attempt genuinely errors. Asserts
    `recover_link_syscall_error(&err, TAR_SYMLINK_MARKER).is_some()` only (no specific kind/code —
    which occupant-shape fails on isn't load-bearing for what this pins).

  Both were cross-checked against `tar-0.4.46`'s actual source
  (`~/.cargo/registry/src/*/tar-0.4.46/src/entry.rs`) before writing: the hard-link wrap at
  `entry.rs:546-556` renders `"{err} when hard linking {src} to {dst}"` and the symlink wrap at
  `entry.rs:558-575` renders `"{err} when symlinking {src} to {dst}"`, exactly `TAR_HARDLINK_MARKER`/
  `TAR_SYMLINK_MARKER`, confirming the fixtures already in the file model the real shape correctly.

  **Red-proof (acceptance criterion 2):** with the new tests present and passing, changed
  `TAR_HARDLINK_MARKER` from `" when hard linking "` to `" when HARDLINKING "` (simulating a `tar` bump
  that reworded its wrap — the real crate's own text is untouched, so this models exactly what a real
  reword would do to the recovery side) and reran
  `cargo test --lib archive::tests::cpe1829_recover_link_syscall_error_pins_against_the_real_tar_crate_hardlink`.
  Result: **red**, exit 101 —

  ```
  test archive::tests::cpe1829_recover_link_syscall_error_pins_against_the_real_tar_crate_hardlink ... FAILED
  ---- archive::tests::cpe1829_recover_link_syscall_error_pins_against_the_real_tar_crate_hardlink stdout ----
  thread '...' panicked at src\archive.rs:6470:17:
  the REAL tar-0.4.46 crate's wrap did not carry TAR_HARDLINK_MARKER (" when HARDLINKING ") where
  recover_link_syscall_error expects it — an upstream tar release reworded its wrap text, which is
  exactly the silent-revert CPE-1829 exists to catch. Real wrapped error: failed to unpack
  `...\out\hard`
  test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2372 filtered out
  ```

  confirming this test (unlike every existing one) actually depends on the real crate's current wording.
  Reverted the constant to `" when hard linking "`; reran the same test:

  ```
  test archive::tests::cpe1829_recover_link_syscall_error_pins_against_the_real_tar_crate_hardlink ... ok
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2372 filtered out
  ```

  **green** again. (Every pre-existing test in the file stayed green throughout the reword — confirmed by
  running the full `archive::` module both before and after — exactly as the ticket predicted: they pin
  against the double, which was reworded in lockstep with the constant, not the real crate.)

  **Doc note landed** (acceptance criterion 3): added a paragraph to `recover_link_syscall_error`'s doc
  naming `entry.rs:514`/`:522` (`"hard link listed for …"` / `"symlink destination for …"`, both
  `other(&format!(..))` over the raw attacker-controlled 512-byte header) and stating explicitly that
  their current harmlessness — `ErrorKind::Other` plus `tar_entry_refusal`'s pre-check — is "a coincidence
  of today's `ErrorKind` choice and today's pre-check, not a structural property of this function," per
  the ticket's instruction not to let a future reader re-derive or assume it's guaranteed.

  **Assumption logged:** the symlink real-crate leg is Unix-only by `#[cfg]`, not attempted-then-skipped
  on Windows — per the ticket's explicit instruction to say so rather than write a leg that silently
  skips. Judgment call: this is stated as a doc comment on the test plus this Work Log entry, not a
  runtime `if cfg!(windows) { return; }` inside an unconditional test, because the latter is exactly the
  "silently skips" shape the ticket warns against — a `#[cfg(unix)]`-gated test simply does not exist on
  Windows, which `cargo test`'s own output makes visible (filtered-out count), rather than reporting a
  false pass.

  `cargo clippy --all-targets -- -D warnings` and `cargo clippy --all-targets --features index -- -D
  warnings` both clean; `cargo test` (whole crate, default features): 2365 passed, 8 ignored (pre-existing,
  unrelated), 0 failed; `cargo test --features index` (archive module): 95 passed, 0 failed.
