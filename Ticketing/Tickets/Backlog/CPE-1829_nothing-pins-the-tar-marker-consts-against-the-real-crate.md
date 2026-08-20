---
id: CPE-1829
title: nothing pins the tar link-marker constants against the real crate, so a tar bump silently reverts CPE-1813
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
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
