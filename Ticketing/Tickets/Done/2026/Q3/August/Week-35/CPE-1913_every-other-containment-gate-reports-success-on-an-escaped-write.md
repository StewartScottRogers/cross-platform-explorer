---
id: CPE-1913
title: every other containment gate in cpe-server reports success on an escaped write — archive, transfer, revert and copilot all check-then-create with no landing check
type: bug
priority: High
status: Done
tags: ready
estimate: L
created: 2026-08-26
---

## Summary

CPE-1896 added `landed_inside` to the backup engine — a post-write check that asks where the bytes
actually went. It is **the only one in the crate.** Four other subsystems do check-then-create with the
same window class and no landing check at all, and all four reach the same silent-success shape:

| site | check → write gap | success shape on escape | who chooses the path |
|---|---|---|---|
| `archive::entry_dir_action` (`archive.rs:838`) | **0 syscalls** on the zip leg; the *entire rest of the archive* on the one-shot tar leg (dir entries deferred to a second pass, `tar_unpack_with:2781` → `:2818`) | `Ok(ArchiveReport { done: 1, errors: [] })` | the archive |
| `archive::entry_sink_action` (`archive.rs:764-809`) | ≥2, unbounded in tree depth; then a plain by-path `fs::File::create` at `:3589` — not `create_new`, not `O_NOFOLLOW` | same | the archive |
| `transfer::download_tree` (`transfer.rs:651`) | 4 local syscalls **plus a full remote file-body fetch** (`provider.read` at `:806`) before `fs::write` at `:807` | `files += 1`, `Ok(n)` — **no per-entry result channel at all** | **the remote server**, every segment |
| `revert_engine::apply_write` (`revert_engine.rs:852`) | ~6-7; the only post-write `canonicalize` is inside the `map_err` closure at `:961` — the *failure* path, for wording only | `report.applied += 1`, `skipped: []` | checkpoint-manifest JSON, against the user's live tree |
| `copilot::apply_op` (`copilot.rs:355`) | 2 (Mkdir) to ~9-10 (Move/Copy) | `OpResult { ok: true, error: "", outcome: Applied }` | LLM plan paths in a user-confirmed folder |

**`transfer::download_tree` is the worst of the five** and should be done first: the widest window (an
entire network transfer sits inside it), the most attacker-controlled name source (a remote SFTP/FTP/
WebDAV server chooses every path segment), and **no per-file reporting channel to be honest in even if
it detected the problem.**

**`archive::entry_dir_action`'s zip leg has a literally zero-syscall gap** — which sounds safe and is
not, because the tar leg defers directory entries to a second pass, putting the whole remainder of the
archive inside the window.

And `archive.rs:426-435` **already records the pre-race version of exactly this shape**
(`landed_outside=TRUE  Ok(ArchiveReport { done: 1, errors: [] })`). The file knows the shape. The fix
was never generalised.

## Acceptance criteria

- [ ] Give `transfer::download_tree` a per-entry result channel first. It currently cannot report a
      per-file failure at all, so no containment fix there can be honest until this exists. Treat that as
      its own deliverable and land it before the guard.
- [ ] Add a landing check to each of the five sites, or — better — extract the one `landed_inside` proved
      out in `backup.rs` into a shared helper so there is one implementation and five call sites rather
      than five implementations. This ticket exists because a fix was written once and not generalised;
      do not repeat that.
- [ ] Fix `archive.rs`'s by-path `fs::File::create` at `:3589` to the `create_new` / no-follow pair the
      rest of the crate uses (CPE-1718's `create_slot_refusal` + `create_exclusive`).
- [ ] Red-proof **each** site with its own harm test asserting on the filesystem — the bytes arriving
      outside the root — never on the returned `Result`. Assert the escape is reported as a failure.
- [ ] Consider splitting this into five tickets once the shared helper exists. It is filed as one because
      the diagnosis is one; the work plausibly is not.

## Notes

Filed 2026-08-26 by CPE-1896's independent Security Auditor, which spot-checked all five sites rather
than the two it was asked for.

Related: **CPE-1896** (the backup landing check, the only one that exists), **CPE-1912** (a junction
inside the root, no race required), **CPE-1898** (the source leg), **CPE-1889** (the static parent
containment), and the resolve-before-write family: CPE-1744/1759 (archive), CPE-1742 (transfer),
CPE-1750 (copilot), CPE-1623 (batch media).

Note the distinction that matters for triage: a previous audit confirmed all five sites **do** check
containment before `create_dir_all`, and that finding was correct. This ticket is not that they skip the
check — it is that the check happens before the write and nothing verifies afterwards, so an escape that
wins the window is reported as a success. Same shape as CPE-1896, four more places.

## Work Log

- **2026-08-27 (Worker) — SCOPE: THREE of the five legs converted, two left, and which is which is
  stated first rather than buried.** Converted: `transfer::download_tree`, `revert_engine::apply_write`,
  and the **zip** extraction loop (`archive::extract_zip_archive_stream`, which both the one-shot and
  the streamed zip paths use, encrypted variants included). **Not converted:** the **tar** leg
  (`tar_unpack_with`) and the **7z** leg (`extract_7z_safe`/`_stream`), and `copilot::apply_op`. Reasons
  in "what was left and why" below. The ticket's own AC5 ("consider splitting this into five tickets
  once the shared helper exists") is now actionable: the helper exists and the remainder is
  self-contained work.

- **2026-08-27 (Worker) — CPE-1912 IS CLOSED, established by testing rather than by reading.** The
  first thing this branch did was stage CPE-1912's fixture verbatim against current `main`:
  `dst/Photos -> dst/Trash`, a junction *inside* the destination, no race, and a plan naming
  `Photos/holiday.jpg`. Landed as
  `backup::tests::cpe_1912_a_junction_inside_the_destination_never_silently_redirects_a_subtree`, with a
  liveness assertion that writes through the junction first so an inert fixture cannot pass.

  - **Green on current `main`:** `ok: false`, and the refusal names the component —
    `the path component "Photos" is a link (a symlink, junction or other reparse point)`. Nothing in
    `dst/Trash`.
  - **Red-proofed by substituting the pre-CPE-1896 open** (`batch_media::open_no_follow(dst)` in place
    of `open_beneath::create_beneath`), which reproduced CPE-1912's report byte for byte:

    ```text
    OpResult { path: "...\dst\Photos\holiday.jpg", ok: true, error: "", outcome: Applied }
    trash_has_photo = true
    ```

  So the closure is CPE-1896's per-component walk, not an accident of the fixture: the walk refuses a
  name surrogate at **every** component and never asks where the path ends up, which is why "both paths
  are inside the root" — the observation CPE-1912 is built on — stopped mattering. **Retire CPE-1912**,
  noting two of its ACs are answered rather than dropped: its AC4 (what a *legitimate* junction inside a
  backup destination should do) is decided as **refuse, naming the component**, and its AC5 (the restore
  mirror) is covered by construction, because Restore is the same `apply_backup_plan_walk` with the roots
  swapped and therefore the same walk.

- **2026-08-27 (Worker) — the per-leg enumeration, derived from the code rather than trusted from the
  ticket. All five still had the defect; none had been fixed incidentally.**

  | leg | what it did before | status |
  |---|---|---|
  | `transfer::download_tree` | ancestor-canonicalize containment, `create_dir_all`, leaf `symlink_metadata`, `name_links` — then `provider.read` (a whole network fetch) and `fs::write(&local, ..)` | **converted** |
  | `revert_engine::apply_write` | `safe_target`'s `confined_to`, `create_dir_all(parent)`, then `copy_file_onto_no_follow` opening `target` by path | **converted** |
  | `archive::extract_zip_archive_stream` (rows 15/16, plain + encrypted, one-shot + streamed) | `entry_dir_action`/`entry_sink_action` by path, `create_dir_all(parent)`, `fs::File::create(&out)` | **converted** |
  | `archive::tar_unpack_with` (rows 21/22) | `tar_entry_refusal` → `entry_sink_action` by path, then the `tar` crate's own `unpack_in` | **left** |
  | `archive::extract_7z_safe`/`_stream` (rows 19/20) | `sevenz_entry_slot_action` → the same two by-path helpers, then `sevenz-rust`'s extract callback | **left** |
  | `copilot::apply_op` | `confined_to` + `same_place`, then `create_dir_all` / `fs::rename` / `copy_recursive` / `trash` | **left** |

  One correction to the ticket's framing, worth recording because it changes what "fix it" means:
  CPE-1881 merged an hour before this work started and **already gave `download_tree` its per-entry
  result channel** (`DownloadReport::skipped`, plus the pre-existing `undelivered`). AC1 was therefore
  already satisfied when this branch opened; what was missing was not the channel but a guard honest
  enough to put something in it.

- **2026-08-27 (Worker) — the shared helper, which is the whole of AC2.** CPE-1896's gate was welded to
  a source-file-to-destination-file copy, which is why four legs with bytes from a *stream* could not
  reuse it. Split out as **`fsutil::claim_destination_handle(dst, wording, open_dst)`**: it runs the
  caller's open (in production always `open_beneath::create_beneath`), then asks the returned **handle**
  whether the object is a name surrogate, a directory, or a second name for a file elsewhere, then
  truncates. `copy_file_onto_destination_handle` is now that function plus the source-file half.
  One implementation, four call sites (backup, restore/revert, download, zip extract).

  Three supporting pieces:

  - **`open_beneath::create_dir_beneath`** — the directory twin of `create_beneath`, for the callers
    that must materialise a directory *for its own sake* (an archive's directory records, a remote
    tree's empty folders) and were reaching for `fs::create_dir_all`. Both arms of `sys` grew a shared
    `descend`, so the file leg and the directory leg cannot drift about what a traversable component is.
  - **`open_beneath::Refusal { why, policy }`** — the refusal now says *which kind of answer it is*.
    `policy: true` means "not writing is the correct outcome" (a link, a hard link, a directory in the
    way); `false` means the entry was not delivered and nobody chose that. Each leg already had two
    buckets with exactly this meaning (`ArchiveReport::skip` vs abort; `DownloadReport::skipped` vs
    `undelivered`; revert's permanent vs transient) and each was deriving the answer from *somewhere
    else*. Carrying it as data rather than in the wording is deliberate: CPE-1896 round 4 shipped two
    assertions that matched a phrase present in every refusal's shared boilerplate, so they passed for
    any failure at all.
  - **`RootDir` carries a noun** ("backup destination" / "download folder" / "extraction folder" /
    "folder being restored"), so one sentence template does not tell someone extracting a zip about a
    backup destination they never chose.

- **2026-08-27 (Worker) — no new instances of CPE-1929, and the removals are the evidence.** Each leg's
  by-path probes were **deleted**, not kept in front of the handle guards. A path question standing in
  front of a handle question answers first, which makes the handle guard deletable with the suite still
  green — the shape that hid a live guard behind an `is_symlink` check for four rounds of PR #1043.

  - `transfer`: `existing_ancestor`, `AncestorProbe`, `classify_ancestor_probe`, `LeafProbe`,
    `classify_leaf_probe` and their four unit tests are gone (~200 lines). `guarded_join` stays — it is
    a *name-shape* rule, not containment, and it is what discharges `create_beneath`'s recorded
    obligation that a caller filter Win32-unstable names itself.
  - `revert_engine::apply_write` calls `safe_segments` instead of `safe_target`: the two differ in
    exactly one thing, `confined_to`, which the walk now answers atomically. `apply_delete` and
    `snapshot_capture::restore` keep `safe_target` unchanged — neither has a walk, so `confined_to` is
    still the best answer available to them.
  - `archive`'s zip loop no longer calls `entry_sink_action`/`entry_dir_action`. Both stay live and
    correct for the tar and 7z legs, and `rows_15_to_20_…` now asserts a different marker per leg for
    exactly that reason.

  One consequence found by doing this rather than reasoning about it: revert's refusal **classifier**
  had to move onto `Refusal::policy`. It used to decide permanence with `symlink_metadata(&target)` —
  which *follows* the junction that caused the refusal, finds an ordinary file on the far side, and
  would have reported a containment refusal as **transient**, telling the user to re-run something that
  can never succeed. That is CPE-1845's own loop, reached through a door CPE-1913 opened.

- **2026-08-27 (Worker) — red-then-green for every new guard. Each harm test asserts on the filesystem
  first, before any `Result` is inspected, and each runs the junction in BOTH directions — pointing
  outside the root and pointing at another folder inside it — because only the second distinguishes the
  new guard from the containment check it replaced.**

  | test | sabotage | red |
  |---|---|---|
  | `backup::…cpe_1912_a_junction_inside_the_destination_never_silently_redirects_a_subtree` | `create_beneath` → `open_no_follow` | `ok: true`, photo in `dst/Trash` |
  | `transfer::…cpe_1913_a_junction_inside_the_download_folder_never_redirects_an_entry` | `create_beneath`/`create_dir_beneath` → `open_no_follow`/`create_dir_all` | bytes in `outside` (outside leg) **and** in `dl/other` (inside leg) |
  | `revert_engine::…cpe_1913_a_junction_at_an_interior_component_never_redirects_a_restored_file` | same | checkpoint bytes in the junction's target, both legs |
  | `archive::…cpe_1913_a_junction_inside_the_extraction_folder_never_redirects_an_entry` | same | archive bytes in the junction's target, both legs |

  Each inside-the-root leg was reddened separately by restricting the loop to `[false]`, so "the outside
  case reddens" cannot stand in for "the inside case reddens".

  **Two existing tests were re-aimed rather than deleted, and the reason is the same in both.**
  `cpe_1857_the_transfer_gate_refuses_when_it_cannot_read_the_link_count` and
  `cpe_1857_an_unreadable_probe_refuses_the_entry_rather_than_writing_it` staged their fixture with
  `ProbeInjection`, a test seam on the **path** probe `batch_media::name_links`. That probe is no longer
  in either decision — the link count comes off the write handle — so the injection changes nothing.
  Both now arm *both* injections over a genuinely hard-linked slot and assert the refusal happens anyway.
  That is strictly stronger than what they replaced: an injection that no longer changes the outcome is
  evidence the outcome no longer depends on the thing injected. Neither test's HARM assertion changed.

- **2026-08-27 (Worker) — what was left, and why, rather than done shallowly.**

  - **`archive`'s tar leg.** The write is `tar::Entry::unpack_in`, inside the `tar` crate. Converting it
    means reimplementing the crate's unpacker — mode bits, times, symlink and hard-link entries, the
    deferred directory pass — not swapping a call. Its containment guard against a link leading
    *outside* `dest` is unchanged and still correct; what it cannot see is a link pointing *inside*.
  - **`archive`'s 7z leg.** Same shape: the write lives in `sevenz-rust`'s
    `decompress_file_with_extract_fn` callback, which is handed a destination path, not a handle.
  - **`copilot::apply_op`.** Not a byte-writing leg at all. Its five ops are `mkdir`, `rename`, `copy`,
    `move` and `trash`; making those beneath-safe needs `renameat`/`unlinkat` primitives that
    `open_beneath` does not have, which is a module-sized addition rather than a call-site change. It is
    also the least attacker-controlled of the five name sources — LLM plan paths inside a folder a human
    confirmed — which is why it is the one left rather than one of the others.

  The ticket's AC3 (`archive.rs`'s by-path `fs::File::create`) is **done for the zip leg** — it is now
  `claim_destination_handle` over `create_beneath`, which is stronger than the `create_new`/no-follow
  pair the AC asked for, because it is handle-relative as well as no-follow. The tar and 7z legs' own
  `File::create`s live inside third-party crates and go with those legs.

- **2026-08-27 (Worker) — one new failure mode, stated rather than discovered later.** All three
  converted legs now need their destination folder to be **openable for read**, because a directory
  handle is what the walk resolves against. A folder that can be written but not opened used to work and
  now fails loudly with a named reason. It is the same trade CPE-1896 recorded for the backup
  destination, and `src/docs/safety-undo.md` says so to the user.

- **2026-08-27 (Worker) — cross-platform, via CPE-1896's extraction harness, and the harness was proved
  able to fail first.** `crates/server` cannot be cross-checked whole from this Windows box (rusqlite's
  bundled build needs a C cross-toolchain that is not installed, and `x86_64-unknown-linux-musl` dies at
  `x86_64-linux-musl-gcc: program not found`). So `open_beneath.rs` was extracted **verbatim** —
  `#[cfg]` attributes and `pub(crate)` visibility included, because a `pub` item is never `dead_code`
  and that is exactly how round 6's harness reported known-bad code clean — into a dependency-free crate
  with caller stubs mirroring the real call sites, and clippied for `x86_64-unknown-linux-gnu` and
  `x86_64-apple-darwin` with `-D warnings`.

  **Red first:** two sabotages, `harness_sabotage_never_used` (round 6's exact shape, proves
  `dead_code`/`-D warnings` is live) **and** a `needless_borrow` reintroduced *inside the `cfg(unix)`
  `descend`* (proves clippy is analysing the Unix arm, which the first sabotage alone would not show).
  Both reported. **Green after**, on both targets.

  That harness earned its keep immediately: the Unix `descend` had three `&sofar` borrows that are
  `needless_borrow` now that `sofar` is a `&mut PathBuf`, and local Windows clippy cannot see a single
  one of them. That is a CI cycle not spent. The generator is `.claude/mkharness.py`, ~60 lines, deleted
  with the branch and reproducible from this entry.

- **2026-08-27 (Worker) — lockfiles: nothing to do, checked rather than assumed.** No dependency
  changed: the branch diff touches no `Cargo.toml` at all, so none of the nine `Cargo.lock` files that
  pin `cpe-server` can be stale for this change. (The enumeration was still run — seventeen lockfiles in
  the repo, nine pinning `cpe-server` — so the conclusion is from the file list rather than from
  memory.) No `specta::Type` struct changed shape either, so `bindings.gen.ts` needs no regeneration;
  `DownloadReport`, `ArchiveReport` and `RestoreReport` are all populated differently and declared
  identically.

- **2026-08-27 (Worker) — guardrails.** `cargo test` in `crates/server`: **2413 passed, 0 failed, 10
  ignored** on the merged-with-`main` state (5 of those passes are `main`'s, from CPE-1891 landing
  mid-branch). `cargo clippy --all-targets -- -D warnings` clean in plain, `--features index` and
  `--features specta`; `cargo check` clean in `src-tauri`; `npm run check` 0 errors 0 warnings; the docs
  guard tests (`docs.test.ts`, `sectionDocs.test.ts`) green. `origin/main` was merged into the branch
  and every one of those was re-run on the merged tree, not only on the branch tip — MERGEABLE is not
  the same as green-after-merge.

- **2026-08-27 (Worker) — follow-ups this work produces.** **Retire CPE-1912** (closed, evidence above).
  Split the remainder as the ticket's AC5 anticipated: one ticket for the **tar + 7z** extraction legs
  (they share `entry_sink_action` and both need a third-party unpacker replaced) and one for
  **`copilot::apply_op`** (which needs `renameat`/`unlinkat` in `open_beneath` first, and is worth
  filing as that primitive rather than as the call site).
