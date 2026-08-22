---
id: CPE-1770
title: Trash restore and folder-template creation use a refusal guard that misses a dangling link
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-17
closed:
---

## Problem

Found by the **PR #924 (CPE-1715) review** while auditing every "is this name free?" probe. These three are
the *refusal-shaped* siblings — they guard a write by refusing rather than by picking another name — and
each uses a guard that cannot see a dangling link.

### 1 & 2. Trash restore — `src-tauri/src/lib.rs:2092` and `:2325`

Both use `clobber_refusal` **alone**. A dangling link at the original path reads free, so `restore_all`
lands on it — restoring the user's file *through* a link to somewhere they never chose, or destroying the
link.

What makes this one worth calling out: the existing safety net does **not** catch it. The CPE-1710 clippy
ban and the `half_applied_rename_guards_are_rejected` scan both miss these sites, because the write happens
inside the `trash` crate rather than through an `fs::rename` the scan can see. The guard that was supposed
to make this class impossible has a blind spot exactly here.

### 3. Folder templates — `crates/server/src/folder_template.rs:176`

Uses `clobber_refusal` + `fs::write`. But this is a **create** site, not a clobber site, so it wants
`create_slot_refusal` (CPE-1718). Using the wrong member of the guard family means the check it performs is
not the check the call site needs.

## Why this is filed separately rather than folded into CPE-1715

CPE-1715's acceptance criteria named `unique_target` and `resolve_conflict` and it satisfied both. Leaving
these unrecorded is how the class keeps coming back — the same argument CPE-1705 makes about twelve copies
of one check.

## What to do

- Route all three through the correct refusal guard for their shape, and say in each case which one and why.
  The distinction that matters: a *clobber* site is about replacing something known to be there; a *create*
  site is about a name that must be free. They are not interchangeable.
- For the trash sites, decide what should happen when the original path is occupied by a dangling link:
  refuse and tell the user, or restore alongside under a picked name. Either is defensible; leaving it to
  land on the link is not. Record the decision.
- **Extend the scan, not just the sites.** `half_applied_rename_guards_are_rejected` missed these because
  the write is behind a crate boundary. A fix that repairs three call sites and leaves the scan blind means
  the fourth one gets missed the same way. Widen the scan to cover writes that go through the `trash` crate,
  or state explicitly why it cannot and what covers them instead.

## Acceptance criteria

- [x] Both trash-restore sites treat a dangling link (and an NTFS junction) at the original path as
      occupied, and behave per the recorded decision.
- [x] `folder_template.rs:176` uses `create_slot_refusal`, with the reason recorded at the call site.
- [x] Each has a test asserting the **harm** — where the restored bytes actually landed, whether the link
      survived — **before** unwrapping the `Result`. This family fails by succeeding, so an assertion after
      an `unwrap` is unreachable exactly when it matters.
- [x] The guard scan is widened to cover the `trash`-crate write path, or its inability to is documented
      with what covers those sites instead.
- [x] Reverting each fix reds a **distinct** test naming the specific site and harm.
- [x] Tests clean up via a `Drop` guard armed **before** the assertions (CPE-1693).
- [x] The junction leg is covered, not only the symlink leg — an unprivileged Windows runner stages a
      junction, and `remove_file` refuses one with PermissionDenied while `remove_dir` succeeds.

## Notes

Found by the Reviewer on **PR #924 / CPE-1715**, 2026-08-17, during the batched sprint. Related: CPE-1715,
CPE-1769 (the name-picking siblings), CPE-1710 (the rename-guard ban with the blind spot), CPE-1718
(`create_slot_refusal`), CPE-1705.

## Work Log

**2026-08-21** — All three sites fixed, branch `cpe-1770-trash-restore-dangling-link` off latest
`origin/main` (head `b5a0fbda`, after CPE-1769/#978 merged). Read CPE-1769's full diff first, per the
Foreman's brief, and reused its helpers rather than inventing a fourth way to ask "is this name free?" —
but the two ticket families are different **shapes**: CPE-1769's three sites are name-*picking* loops
(advance past an occupied candidate); this ticket's three sites are *refusal*-shaped (refuse the whole
write). That distinction drove which existing `fsutil` member each site actually needed:

- **Trash restore** (`src-tauri/src/lib.rs`, `restore_from_trash_impl` and `restore_trash_items_impl`) —
  both used `clobber_refusal` alone. Neither calls `std::fs::rename` itself (the write happens inside the
  `trash` crate, across a crate boundary this repo does not control), so the existing rename-shaped combo
  (`rename_slot_refusal`/`symlink_slot_refusal`, whose wording is specifically about what a `rename`
  itself does to a link) was the wrong fit. Added `clobber_refusal_link_aware` to `crates/server/src/fsutil.rs`
  — `clobber_refusal`'s own body, with the probe swapped from a bare `target.try_exists()` to
  `name_pick_slot_probe` (built by CPE-1715, reused by CPE-1769) — so both trash sites keep the SITE's own
  wording for a dangling link, exactly as they already did for a real occupied file, rather than asserting
  what `trash::os_limited::restore_all` will do to it (which this crate cannot promise and does not
  control).
  - **Decision, recorded as the ticket asked**: refuse, not restore-alongside. Read the `trash` crate's own
    source (`trash-5.2.6`) to inform this rather than guessing. On **Linux** (`freedesktop.rs`), the
    crate's own restore already uses `OpenOptions::new().create_new(true)`/`fs::create_dir` at the
    original path — both refuse on a dangling symlink there (POSIX `O_EXCL` never follows the final
    component, dangling or not), so Linux already had an independent write-time backstop, same shape as
    one of CPE-1769's findings. On **Windows** (`windows.rs`), `restore_all`'s own pre-check is
    `path.exists()` — which ALSO follows symlinks and misses a dangling one — and the actual move goes
    through `IFileOperation::MoveItem`, an opaque Shell API this crate does not control. **Measured, not
    guessed**: with the fix reverted, `restore_from_trash_impl` against a real dangling link on this
    Windows dev machine returned `ok=true`, and the link was GONE afterward — `symlink_metadata` on the
    original path came back `is_symlink() == false` (a real file sits there now, matching the restored
    probe's identity) and nothing appeared at the link's own phantom target. So on Windows this is a real,
    unguarded destructive clobber of the user's link with no independent backstop — not merely an
    unconfirmed-skip nuance like one of CPE-1769's sites — which is why "refuse" (matching how both sites
    already treat a real occupied file) rather than "restore alongside" is the right call: it is symmetric
    with existing behaviour and platform-independent, since this crate's own guard is now the only thing
    guaranteed to run before `restore_all` on every OS.
- **Folder templates** (`crates/server/src/folder_template.rs:176`, `stamp_nodes`'s `Node::File` arm) —
  swapped `clobber_refusal` for `create_slot_refusal` (CPE-1718), exactly as the ticket named. This is a
  CREATE site (`fs::write` two lines below), not a clobber site: `create_slot_refusal` checks the link
  question BEFORE occupancy (the opposite order from a rename-destructive site) because at a create site a
  **live** link is just as much a write-through hazard as a dangling one — `fs::write` follows either and
  the caller believes it wrote `file` when it actually wrote through to wherever the link points. One
  existing test (`cpe_1705_stamp_refuses_a_file_slot_it_cannot_stat`) needed its assertion updated: the
  ACL-denied-slot message now comes from `create_slot_refusal`'s link-check-first `Unknown` arm
  ("could not check WHETHER … IS A LINK") rather than `clobber_refusal`'s own ("could not check what is AT
  …") — same fail-closed verdict, different wording source, so the assertion was corrected rather than
  loosened.
- **Guard scan widened, not just the sites** — `half_applied_rename_guards_are_rejected`
  (`crates/server/src/fsutil.rs`) generalised its window-scan mechanism (`renames_within_window` →
  `write_within_window(lines, from, marker)`, parameterised on the destructive-write marker) and now also
  flags a bare `clobber_refusal(` immediately above `restore_all(` — the trash-crate write these two sites
  hid behind, textually invisible to a scan anchored only to `fs::rename(`. A corrected site calls
  `clobber_refusal_link_aware`, which the scan does NOT flag (`"clobber_refusal_link_aware(".contains("clobber_refusal(")`
  is false — the character after `clobber_refusal` there is `_`, not `(`), and a `trash_boundary_calls`
  counter (mirroring the existing `combined_calls` counter for `rename_slot_refusal`/`rename_into_slot`)
  asserts `>= 2` so a regression that silently reverts one of the two sites is caught even without a
  dangling-link fixture on the CI runner. Documented plainly what this does NOT claim: it is one more named
  marker for the one crate boundary this ticket found blind, not a general answer to "what if a write hides
  behind some other crate" — `disallowed-methods` is the only structural (not textual) guarantee, and it
  only covers `std::fs::rename`.
- **Tests** — 5 new (`clobber_refusal_link_aware_covers_the_dangling_link_clobber_refusal_alone_cannot_see`
  and `the_scan_also_finds_a_bare_guard_above_a_trash_restore_write` in `fsutil.rs`;
  `cpe_1770_stamp_refuses_a_dangling_link_at_the_file_slot_instead_of_writing_through_it` in
  `folder_template.rs`; `cpe_1770_restore_from_trash_refuses_when_a_dangling_link_occupies_the_original_path`
  and `cpe_1770_restore_trash_items_refuses_when_a_dangling_link_occupies_the_original_path` in
  `src-tauri/src/lib.rs`), each staging a dangling link via the existing `fsutil::make_dangling_link` and
  asserting the harm on disk (nothing at the link's phantom target; the slot/link's own `symlink_metadata`
  survives) **before** trusting the `Result`, per the ticket's own framing of this bug family. `ScratchDir`/
  `TempDir` drop guards armed at construction throughout, no trailing `remove_dir_all` racing an early
  assertion.
- **Red-proofed individually**, one line at a time, then reverted:
  - `fsutil.rs`: `clobber_refusal_link_aware`'s `name_pick_slot_probe(target)` → `target.try_exists()` —
    reds `clobber_refusal_link_aware_covers_the_dangling_link_clobber_refusal_alone_cannot_see`
    (`left: None, right: Some("occupied")`).
  - `folder_template.rs`: `create_slot_refusal(...)` → `clobber_refusal(...)` at the `Node::File` write —
    reds `cpe_1770_stamp_refuses_a_dangling_link_at_the_file_slot_instead_of_writing_through_it` (`stamp`
    returned `Ok` and actually wrote through the link).
  - `src-tauri/src/lib.rs`, `restore_from_trash_impl`: `clobber_refusal_link_aware(...)` →
    `clobber_refusal(...)` — reds `cpe_1770_restore_from_trash_refuses_when_a_dangling_link_occupies_the_original_path`.
    Diagnosed the actual harm with a temporary print (removed before commit): `ok=true`, `error=""`,
    `phantom_target.exists()=false`, `symlink_metadata(probe).is_symlink()=Ok(false)` — the real Windows
    Shell restore silently **destroyed the link and replaced it with the restored file**, not a
    write-through-to-elsewhere.
  - `src-tauri/src/lib.rs`, `restore_trash_items_impl`: same swap — reds
    `cpe_1770_restore_trash_items_refuses_when_a_dangling_link_occupies_the_original_path`.
  - The scan widening was red-proofed incidentally during development: with only ONE of the two trash
    sites fixed, `half_applied_rename_guards_are_rejected` failed naming exactly the still-unfixed site
    (`lib.rs:2695`) and nothing else — confirming the new marker finds a real half-applied site rather than
    matching everything.
- **Link shape actually exercised, checked with `fsutil reparsepoint query`, not inferred from the
  PowerShell cmdlet** (CPE-1769's own lesson, applied here rather than re-derived the wrong way): staged a
  link the same way `make_dangling_link` does (`os.symlink`/`std::os::windows::fs::symlink_file`) and
  queried the artefact directly. Reparse Tag Value `0xa000000c` = `IO_REPARSE_TAG_SYMLINK` — **this
  machine's tests exercise the symlink leg** (Developer Mode is enabled here, so the unprivileged-create
  flag lets `symlink_file` succeed even though the PowerShell `New-Item -ItemType SymbolicLink` cmdlet
  would fail without admin). The junction fallback rests on CI, unverified locally — same situation as
  CPE-1769, recorded here rather than assumed.
- **Gates**: `cargo clippy --all-targets -- -D warnings` clean for `crates/server` and BOTH `src-tauri`
  feature modes (default and `sidecar-platform`). `cargo test` clean: `crates/server` 2287 passed, 0
  failed, 4 ignored (lib) plus all other test binaries green; `src-tauri` default features 212 passed, 0
  failed; `src-tauri --features sidecar-platform` 267 passed, 0 failed. No `specta::Type` struct touched,
  so no bindings regen was needed; no dependency changed, so neither `Cargo.lock` needed updating.
