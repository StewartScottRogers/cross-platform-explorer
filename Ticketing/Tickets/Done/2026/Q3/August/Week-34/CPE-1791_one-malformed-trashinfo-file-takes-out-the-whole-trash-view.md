---
id: CPE-1791
title: one malformed .trashinfo file takes out the whole Trash view — a dependency panic escapes to the caller
type: bug
priority: Medium
status: Done
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

**Correction (2026-08-20 review):** route 1 below, as originally filed, is wrong and is kept here struck
through rather than silently deleted, since the code's own module comment now carries the corrected
version and a reader comparing the two should see why they diverge.

~~1. **A concurrent writer.** `move_to_trash` writes a `.trashinfo` non-atomically — `create_new`, then
   `writeln!("[Trash Info]")`, then `writeln!("Path=…")`. A reader enumerating between those writes
   sees a file whose only line is `[Trash Info]`, which has no `=`. So a user emptying the trash from
   another application, or the desktop environment doing housekeeping, while our Trash view lists it,
   is enough.~~ **Refuted:** `list()` does `.lines().skip(1)`, so a file whose only line is
`[Trash Info]` never even reaches the loop body that panics — it's silently skipped the same way a file
missing `Path=` entirely is. Only a mid-write torn `write()` that splits the literal `"Path="` string
itself would reach the panicking line via a race, which is vanishingly rare.

2. **A different desktop implementation, or a hand-edited file.** The freedesktop trash spec is
   implemented by many tools; nothing guarantees every `.trashinfo` on a user's disk was written by this
   crate. **This is the realistic trigger** — a *static* on-disk condition, not (mainly) a race, which a
   malformed file can persist under indefinitely since nothing ever rewrites it. That mattered for the
   fix design: see "Resolution" below.

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
  call sites (`lib.rs:2083`, `:5681`, `:6624`, `:6651`). **Answered:** no exposure. All four are
  `trash::delete()` → `move_to_trash()`, which only `create_new`s a `.trashinfo` and `writeln!`s to it —
  it never reads or parses an *existing* one, so the parsing panic this ticket is about cannot be
  reached from any of them. (`restore_all`/`purge_all`, used by the restore/empty commands, were also
  checked: they act on already-parsed `TrashItem` fields carried from a prior `list()` call, not on raw
  file content, so they don't share the exposure either — though see "Resolution" below for a related
  but distinct bug those two commands DID have.)

## Resolution (2026-08-20)

Landed as `catch_unwind` + a thread-local-gated panic hook around every production call site that
invokes `trash::os_limited::list()` (`list_trash_impl`, `list_trash_stream`, `restore_from_trash_impl`,
`restore_trash_items_impl`, `empty_trash_impl` — all five; two of the "what to do" bullets above only
named the first two, and the first two review rounds missed the other three too). A caught panic
degrades a *listing* pass to an empty page (self-heals on the next refresh); a *restore/empty* call
instead fails outright with an `Err` — silently treating a caught panic as "the trash is empty" would
misreport every restore target as already emptied, or a purge as having succeeded when it purged
nothing.

**The per-entry-skip design this ticket originally asked for ("show every other entry, only the bad one
skipped") was attempted and abandoned.** A Linux-only "quarantine" layer pre-scanned `.trashinfo` files
for the exact malformed shape, moved ONLY those aside before `list()` ran so it never saw them, then
restored them right after — a true per-entry skip, closer to this ticket's original ask than the
`catch_unwind`-only fallback that shipped instead. It went through three review rounds, and every round
found a NEW correctness bug in the same mechanism, each one arising from moving real files inside the
user's actual trash directory:

1. Not crash-durable — the restore-on-drop doesn't run on SIGKILL/OOM-kill/power loss/an abort, so a
   kill mid-listing could leave a file permanently outside `info/`, invisible to every trash tool and
   surviving "Empty Trash" too.
2. The quarantine destination was keyed by the original filename, so two overlapping listings
   (independent blocking-pool threads — two panes, or a refresh mid-stream) could clobber each other's
   held copy via `rename`'s silent-overwrite semantics.
3. **The one that killed it:** the quarantine guard's lifetime was scoped to the `list()` call, so a
   quarantined file was restored to `info/` *before* `empty_trash_gated`'s purge targets (computed from
   that same `list()` call) had a chance to include it. `empty_trash_impl` would then report `Ok(())`
   while the malformed entry — and its real payload in `files/` — silently survived on disk untouched.
   That is exactly the "one malformed file breaks everything" failure this ticket exists to fix, just
   inverted into a false success instead of a crash. And for the realistic, *static* trigger (route 2
   above), it would have been the **ordinary** outcome of every Empty Trash while any non-conforming
   `.trashinfo` sat on disk, not a rare edge case.

Given each fix attempt added more complexity to a mechanism that had already produced two prior bugs, on
a wide-blast-radius path (deleting a user's trash contents), the decision (recorded in the code's own
module comment above `list_trash_catching_dependency_panics`, `src-tauri/src/lib.rs`) was to drop the
quarantine layer entirely and keep only `catch_unwind`. The promise this ships is smaller than
originally asked — a malformed file makes a listing come back thin rather than showing every other
entry, and makes restore/empty fail rather than partially succeed — but it is honestly kept: no risk of
moving, clobbering, or losing real user data, and a fraction of the code. If per-entry skip is wanted
again later, the three failure modes above are the bar any replacement design has to clear.

Red-proofed (Linux-only, since the panic is Linux-only): one test plants the ticket's original evidence
shape and proves the raw dependency call panics, then proves `list_trash_impl` degrades to an empty
listing instead of crashing. A second test proves the round-3 blocker specifically: `restore_from_trash`,
`restore_trash_items`, and `empty_trash` all fail loudly (never `Ok`) when the dependency panics, and
that a failed Empty Trash call purges nothing.

## Notes

Filed by the Foreman from PR #940's review, 2026-08-19. The reviewer traced the panic to its exact line
in the dependency and identified the non-atomic write sequence that can expose it to a concurrent
reader.

Related: **CPE-1785** (the test-suite half, which redirects each test to a private trash),
**CPE-1693** (the shared-temp-state family), and `CLAUDE.md`'s `list_dir` skip-on-error rule.
