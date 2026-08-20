---
id: CPE-1791
title: one malformed .trashinfo file takes out the whole Trash view — a dependency panic escapes to the caller
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-19
closed:
---

## Problem

`list_trash_impl` (`src-tauri/src/lib.rs:2292-2293`) calls `trash::os_limited::list()` directly. On
Linux that function **panics** at `trash-5.2.6/src/freedesktop.rs:140` on any `.trashinfo` file whose
body contains a line without an `=` — the code does `split.next().unwrap()`.

The panic surfaces through `tauri::async_runtime::spawn_blocking` (`lib.rs:2288`) as an opaque
`JoinError` string. So **one** malformed file among thousands takes out the entire Trash view, with an
error a user cannot act on.

`list_trash_stream` (`lib.rs:2311`) makes the same call and needs the same treatment.

This directly contradicts the rule in `CLAUDE.md`: *"Filesystem commands skip entries they can't read
rather than failing the whole listing — preserve that behavior when editing `list_dir`."* The Trash
view is a filesystem listing; it should skip the entry it cannot parse, not refuse to render.

## How a malformed file gets there

Two routes, neither exotic:

1. **A concurrent writer.** `move_to_trash` writes a `.trashinfo` non-atomically — `create_new`, then
   `writeln!("[Trash Info]")`, then `writeln!("Path=…")`. A reader enumerating between those writes
   sees a file whose only line is `[Trash Info]`, which has no `=`. So a user emptying the trash from
   another application, or the desktop environment doing housekeeping, while our Trash view lists it,
   is enough.
2. **A different desktop implementation.** The freedesktop trash spec is implemented by many tools;
   nothing guarantees every `.trashinfo` on a user's disk was written by this crate.

## Why it is filed separately from CPE-1785

CPE-1785 fixed the *test-suite* exposure by redirecting each test to a private trash directory, and its
PR declined this bullet on the grounds that the trigger "can no longer happen in this test suite at
all". That is true and does not address the exposure above — the production path never had a redirect
and still calls the panicking function.

The reviewer of PR #940 accepted the complexity argument for deferring it (`catch_unwind` plus a panic
hook, applied to two call sites) but required it be recorded as an open production defect rather than
dismissed with a test-scoped rationale. This ticket is that record.

## What to do

- Contain the panic at the boundary rather than trusting the dependency: `catch_unwind` around the
  `list()` call, with a panic hook that suppresses the default stderr spew, and map a caught unwind to
  a skipped entry plus a logged diagnostic naming the file.
- Better where feasible: enumerate and parse the trash entries ourselves so a malformed file is an
  `Err` for that entry rather than an unwind for the process. Weigh that against the lean-core
  guardrail before adding code.
- Apply to **both** `list_trash_impl` and `list_trash_stream`; a fix to one leaves the other exposed.
- **Red-proof it** per the Evidence Rules in `Ticketing/wiki.md`: plant a `.trashinfo` containing a
  single line with no `=` in a redirected trash directory, show the listing failing entirely before the
  fix, and showing every other entry with the bad one skipped after.
- Consider whether the same "a dependency may panic" exposure exists at the other `trash::` production
  call sites (`lib.rs:2083`, `:5681`, `:6624`, `:6651`).

## Notes

Filed by the Foreman from PR #940's review, 2026-08-19. The reviewer traced the panic to its exact line
in the dependency and identified the non-atomic write sequence that can expose it to a concurrent
reader.

Related: **CPE-1785** (the test-suite half, which redirects each test to a private trash),
**CPE-1693** (the shared-temp-state family), and `CLAUDE.md`'s `list_dir` skip-on-error rule.
