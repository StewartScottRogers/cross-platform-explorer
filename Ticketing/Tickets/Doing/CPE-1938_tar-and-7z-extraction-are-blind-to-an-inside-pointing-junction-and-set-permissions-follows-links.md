---
id: CPE-1938
title: tar/7z extraction is blind to an inside-pointing junction, and the `#[cfg(unix)]` permission pass follows links — an archive can chmod (setuid included) outside the root
type: bug
priority: High
status: In Progress
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Two residuals from PR #1050's independent Security Audit (CPE-1913). They were deliberately scoped
**out** of that PR — it fixed the zip leg and deliberately left tar/7z alone, for the documented
reason that those two need a third-party unpacker replaced — but both are real and neither is
recorded anywhere else.

## F-A: tar (measured) and 7z (inferred) do not see an inside-pointing junction

CPE-1913 gave the **zip** leg a handle gate. The **tar** and **7z** legs still resolve entry
destinations by **path**, so a junction planted at an entry's name inside the extraction root is
followed and the payload lands wherever it points.

Measured on the tar leg:

    [tar junction->outside]  Ok((done 1, skipped 0, errors []))
    -> the victim file outside the root holds the archive payload

The **7z** case is **inferred from the shared shape, not demonstrated** — the auditor says so
explicitly and this ticket repeats it rather than laundering it into a measurement. **Demonstrate it
before fixing it**, and if it turns out 7z is already safe for a different reason, record why.

## F-B: the `#[cfg(unix)]` permission pass follows links, with an archive-chosen mode

After writing an entry, the extraction loop makes a **path-addressed** `fs::set_permissions` call
under `#[cfg(unix)]`. `set_permissions` **follows symlinks**. So a link swapped in between the write
and the chmod re-targets the mode change at whatever the link points to — outside the root — with a
mode **the archive chooses**, and that mode can include **setuid**.

This is the same race the whole of CPE-1896/CPE-1913 exists to close, in the one call that survived
the conversion because it changes metadata rather than writing bytes. PR #1050 counted it and named
it: **two** path-addressed writes remain in that loop — this one and the symlink-entry branch.

The fix shape already exists in this repo: hold the handle from the write and set the mode through
it (`fchmod`-equivalent) rather than re-addressing by name. `open_beneath.rs` is the seam.

## Acceptance criteria

- [ ] **Demonstrate 7z** before changing it. Tar is measured; do not fix 7z on the strength of the
      analogy alone (this repo's recurring defect is a check that looks stronger than it is).
- [ ] Give the tar leg the same handle gate the zip leg got in CPE-1913, or record concretely why the
      third-party unpacker makes that impossible and what the interim containment is. "Blocked on the
      unpacker" is an acceptable answer **only** if it names the blocker and the mitigation.
- [ ] Convert the `#[cfg(unix)]` permission pass to a **handle-addressed** mode change. It is a
      smaller, self-contained fix than the tar leg and should not wait for it.
- [ ] Cover the **symlink-entry branch** — the second surviving path-addressed write — or say why it
      is safe.
- [ ] **Red-proof each fix by racing it**, not by reading it. The auditor's racer already exists and
      is the right tool; a fix that cannot be shown to change a losing trial into a refusal has not
      been demonstrated. Prove the harness can go red before trusting its green (CPE-1929's rule).
- [ ] Assert on the **filesystem** — that the victim outside the root is untouched and its mode is
      unchanged — not on an error string.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1050's Security Auditor, which raised these as F2/F3
alongside the F1 regression that PR then fixed. Its third finding became **CPE-1937** (revert delete
destroys files outside the root, with the 596-files measurement).

Related: **CPE-1913** (the zip leg, fixed), **CPE-1896** (the handle-gate family), **CPE-1935**
(a half-extracted folder with no per-entry report — same loop, adjacent symptom), **CPE-1937**
(the sibling destructive finding from the same audit).

## Work Log

### 2026-08-27 — both findings demonstrated, then fixed

**F-A, measured before the fix (Windows 11, a junction planted at `dest/sub`, no privilege needed).**
The ticket's `[tar junction->outside]` shape did **not** reproduce: on this branch all four tar/7z legs
already refuse an outside-pointing link (`entry_sink_action`'s `confined_to`, CPE-1744). The live hole
is the **inside-pointing** one the title names, and it is worse than the ticket says because the 7z
legs report clean success:

```text
[tar  one-shot  junction -> dest/other] Err("failed to unpack `…\out\sub`")
                                        other/leaf.txt = "ARCHIVED LEAF"   <- payload redirected
                                        other/deeper   = created           <- tree shape redirected
[tar  streamed  junction -> dest/other] Err("failed to unpack `…\out\sub`")
                                        nothing extracted at all, ok.txt included   <- denial
[7z   one-shot  junction -> dest/other] Ok(done: 2, skipped: 0, errors: [])
                                        other/leaf.txt = "ARCHIVED LEAF"   <- silent
[7z   streamed  junction -> dest/other] Ok(done: 2, skipped: 0, errors: [])
                                        other/leaf.txt = "ARCHIVED LEAF"   <- silent
```

So **7z is demonstrated, not inferred** — the acceptance criterion the ticket put first.

**F-B, measured before the fix (real ext4 under WSL, not `/mnt/z`).** A thread that replaces `dest/a.txt`
with a symlink to a file outside the extraction folder while the loop is still working through later
entries: `trials=60 swaps=60 MODES_CHANGED_OUTSIDE=60`, victim `0o644 -> 0o777`. 60 of 60, because the
pass was deferred to the end of the archive — the window was the whole rest of the run, not a sliver.

**Fixes.**

- **F-B** — `extract_zip_archive_stream`'s deferred `(path, mode)` drain is gone; the mode is now an
  `fchmod` on the descriptor `claim_destination_handle` returned, applied inline after `io::copy`. The
  deferral was only ever needed for *directory* modes and this loop never recorded one.
- **F-A** — new `entry_component_action`: every directory component of an entry's destination is opened
  by name **relative to the handle of the component above it**, from the extraction folder's own handle
  (`open_beneath::create_dir_beneath`). Wired into all four tar/7z legs, after the existing path
  questions. It is **interim containment, not a handle gate**: `tar::Entry::unpack_in` and
  `sevenz_rust::default_entry_extract_fn` still own the write and still take a path, so a *planted* link
  at a component is refused whichever way it points, and a component *raced* in between the walk and the
  unpacker is not. Replacing the unpackers stays the named blocker; that residual is written at the site
  rather than implied by a green test.
- **Symlink-entry branch** — re-checked; half the standing reason was stale. `unlinkat` exists now
  (`remove_file_beneath`, CPE-1937) but `symlinkat` does not, and converting only the delete would put a
  handle-relative unlink in front of a by-path `symlink` that re-resolves the same components — a guard
  whose predicate could never decide anything. Both halves stay by path, together, with the residual
  stated at the site.

**Red-proofs (CPE-1929's pair, run by hand, full `--lib` suite each time).**

| Sabotage | Result |
|---|---|
| A: `entry_component_action` always returns `Write` | 2427 passed, **1 failed** — the new regression, on the harm assertion. Loop pinned to the outside-pointing legs: **green**. |
| B: `create_dir_beneath`'s `policy` refusal mapped to `Write` | 2427 passed, **1 failed**, same test. |
| C: `confined_to` short-circuited in `entry_sink_action`/`entry_dir_action` | inside-pointing legs **green**, outside-pointing legs **red on the marker** — the walk catches them too, so C changes *which guard answers*, not whether bytes escape. |
| D: F-B fix reverted to the deferred path-addressed drain | **red**, `MODES_CHANGED_OUTSIDE = 19/20` on ext4. |

Neither half of the A/B pair came back green, so the new refusal is reachable *and* decides. No guard
here is shadowed: containment runs first and owns the outside-pointing input, the walk runs second and
owns the inside-pointing one.

**Sensitivity controls, both normal CI tests, neither `#[ignore]`d.**

- `cpe1938_the_by_path_primitives_write_through_a_planted_link_in_both_directions` — runs the unpackers'
  own `create_dir_all` + `File::create` against the identical fixture and asserts the attack **succeeds**.
  All three OSes; `make_dir_link` panics rather than skipping if a link cannot be planted.
- `cpe1938_the_old_path_addressed_mode_pass_chmods_through_a_planted_link` — `#[cfg(unix)]` by
  construction (the pass does not compile on Windows), asserts `chmod(2)` really follows a link here.

**Per-path verdict** (enumerated at run time from `git ls-files`, then grepped for `create_dir_all` /
`File::create` / `set_permissions` under `archive.rs` and its neighbours):

| Path | Verdict | Action |
|---|---|---|
| rows 15/16/23 — the three ZIP loops | **partly defective** — the *file* and *directory* branches are handle-gated (CPE-1913), the **symlink branch was not** | F-B fixed; component walk added to the symlink branch (**CPE-1973**, round 2) |
| rows 21/22 — `tar_unpack`, `extract_tar_stream` | **defective** (inside-pointing) | component walk added |
| rows 19/20 — `extract_7z_safe`, `extract_7z_stream` | **defective** (inside-pointing, silent) | component walk added |
| rows 13/14 — the two `.gz` branches | one archive-named leaf in a user folder, `refuse_link_at_new_file` | unchanged — no component chain exists |
| rows 2–5 — `extract_archive_entry` / `_tar_` / `_7z_` / `extract_rar_entry` | app-owned `temp_extract_target`, exclusive-created per call | unchanged — no user or archive can plant under it |
| rows 6/8–12 — the compress sinks | caller-named `dest`, not an extraction | out of scope |
| `transfer::download_tree` | already opens a root handle (CPE-1913) | unchanged |
| RAR bulk extraction | does not exist — `.rar` is single-entry, STORE-only | n/a |

**New failure mode, deliberate and loud:** the tar and 7z legs now need the extraction folder to be
*openable for read*, the same trade CPE-1913 recorded for ZIP.
`cpe1938_an_unopenable_extraction_folder_aborts_the_tar_and_7z_runs` pins it, and
`cpe1759_an_unreadable_slot_aborts_both_tar_paths_rather_than_being_skipped` was restaged one level down
so it still reaches the guard it names rather than passing on the new one.

**Verification.** `cargo test` green on Windows (2428 lib + integration) and Linux/ext4 (2415 lib);
`cargo clippy --all-targets -D warnings` clean in both feature modes on both platforms. Docs:
`src/docs/explorer-archives.md` gained the inside-pointing-shortcut bullet.

---

## Work Log — round 2 (2026-08-27)

Reviewer returned APPROVE; the Security Auditor returned SEC FINDINGS with a HIGH blocker. Both are
folded in here. Everything below was measured on this branch rather than reasoned about.

### The Reviewer's Linux gap is closed

The Reviewer could not run the Linux legs (no `cc`, no `sudo -n`) and reported the two `#[cfg(unix)]`
F-B tests, the Linux clippy legs and sabotage D as unconfirmed. **A no-sudo toolchain is already staged
at `~/lintools/bin` (gcc-15 via dpkg-extracted debs) and works** — `cpe-server` builds and tests there
in ~37s with `CC=$HOME/lintools/bin/cc`. All five CPE-1938 legs pass on Linux, and everything below was
run on real ext4 with `TMPDIR` pointed off `/tmp` (`/tmp` on WSL is **tmpfs**, which silently
invalidates a "real ext4" label — the Auditor's methodology note, now recorded at the F-B test).

### F1 / CPE-1973 (HIGH) — the ZIP symlink branch had no component walk. Fixed.

Reproduced verbatim on the unmodified branch, real ext4, zip entry `sub/victim` (a link entry), a
**planted** `dest/sub -> dest/other`, a real user file at `dest/other/victim`:

```text
outcome = Ok(ArchiveReport { done: 2, failed: 0, skipped: 0, cancelled: false, errors: [] })
dest/other/victim is now a symlink: true      link target: Some("benign.txt")
its content reads back as: None               <- the user's file was DELETED
```

`create_beneath` is called only in the loop's *file* branch and `create_dir_beneath` only under
`entry.is_dir()`, so the symlink sub-branch reached a by-path `symlink`/`remove_file` with its
components unresolved. `confined_to` canonicalises **through** the plant and truthfully answers
"inside"; `materialise_entry_symlink`'s `AlreadyExists` retry then unlinks a file the archive never
named — so the residual was a **delete**, not the harmless extra link the old note claimed.

**Fix:** `entry_component_action(&root, &name, false)` now runs on that branch before anything by-path
touches `out`. No `symlink_beneath` needed; only the raced case still wants that primitive. It does not
shadow `link_target_action` (CPE-1929): the walk asks whether the entry's *name* stays inside, the other
asks whether the link's *target* escapes, and each is refused only by its own guard.

**Red-proof:** with the new walk forced to `EntrySlotAction::Write`, the regression
`cpe1973_a_zip_symlink_entry_is_never_created_through_a_planted_component_link` reds on the harm
assertion (victim `None` instead of `"USER FILE"`). Green with the walk. Passes on Windows too, where
the plant is a privilege-free junction and the refusal lands before `create_entry_symlink`.

**Two false statements corrected**, both load-bearing and both false in the safe-looking direction: the
per-path row above, and the residual note on `extract_zip_archive_stream` (which had bounded the
exposure to a race and called the exclusive-create retry harmless).

### F2 (MEDIUM) — the Windows fail-open whose backstop this PR removed. Now fails closed.

`open_beneath::sys::name_surrogate_at` was `unwrap_or(false)`, justified in `batch_media.rs` by "a
genuine surrogate is caught one component later by NT itself". True for `create_beneath`, whose descent
is always followed by a leaf open — **not** true for `create_dir_beneath` used as a verification-only
pass in front of a by-path unpacker, where `sub/leaf.txt` is a **one-component** chain with no next NT
open. Flipped to `unwrap_or(true)`. The `None` arm is untestable by construction (nothing can make
`GetFileInformationByHandleEx` fail on a just-opened handle), so this costs nothing observable and
removes the dependency. Both callers now fail closed; the "opposite defaults" split and the sentence
claiming the NT backstop are gone. This is CPE-1933 landing on a doc comment: the claim was about
another site's control flow, and this PR is what falsified it.

### F3 (MEDIUM) — the raced residual is recorded, with its numbers.

Rows 16 and 19–22 of the CPE-1733 table now carry an explicit **residual: a RACED component swap**
marker instead of advertising containment the legs do not have (CPE-1958), and
`entry_component_action`'s "What it is NOT" section carries the Auditor's measurement — 40 trials ×
500 entries, `RENAME_EXCHANGE` so the component is never absent, target **outside** `dest`: tar one-shot
8/40 (10 entries), tar streamed 5/40 (5 entries), against 9/40 and 17/40 with the walk disabled. Planted
100% → 0%; raced narrows ~2–5× and stays open. Also recorded: the **naive** remove-then-create attacker
looks harmless because the vanished component aborts the run, while the atomic one does not.

### F4 (LOW-MED) — the Abort arm is now covered, and kept, with the argument written down.

The Auditor's CPE-1929 pair came back green in both halves (2413/2 either way), so the arm that
escalates a component refusal to whole-archive failure was uncovered.
`cpe1938_a_component_the_filesystem_refuses_for_an_io_reason_stops_the_run` now forces a deterministic
`EACCES` (a `0o555` `dest`, with a verified deny so a root runner gets a loud skip rather than a vacuous
green) and pins the abort. Kept as an abort rather than demoted, argued at the site: `create_dir_beneath`
*creates* missing components, so `ENOENT` means something removed one under a live extraction — the
concurrent-mutation attacker this ticket is about — and the file branch one level down already returns
`Err` on the same `Refusal::policy == false` class, so a Skip would leave two branches disagreeing about
one fact. The wording complaint is real and deliberately **not** fixed here: the sentence is
`open_beneath::refuse`'s, shared by three legs and pinned by tests in all of them.

### Setuid: measured, no longer inferred

Round 1 wrote "inferred, not measured". The `& 0o777` mask is the `zip` **writer's**, not the format's,
so a hand-built STORED archive with external attributes `0o104755` answers it: the setuid bit is
archive-controllable end to end (`dest/a.txt` lands `0o4755`), the old primitive moved a victim outside
the root `0o644 -> 0o4755`, and end-to-end with `main`'s deferred drain restored,
`trials=20 swaps=20 MODES_CHANGED_OUTSIDE=20 SETUID_OUTSIDE=20` — against `20 swaps, 0 escapes,
0 setuid` on this branch. So F-B was a **privilege-escalation primitive on `main` at 20/20**. Scope
stated honestly at the site: `chmod(2)` only succeeds for the file's owner, so this is fatal when
extracting as root or a service account and a same-user integrity problem otherwise. Sabotage D
re-ran at **20/20**, hotter than round 1's 19/20, which is now noted as a floor rather than a rate.

### Three doc corrections from the Reviewer

1. **The count reconciliation was stale on two of its three numbers.** Re-derived over the production
   half of `archive.rs` (everything above `#[cfg(test)]`, comment lines excluded): **6** `create_dir_all`
   (2 in row 1, 3 in row 17, 1 in row 21 — row 18 contributes **none** since CPE-1913), **11**
   `File::create`, **2** exclusive `fs::create_dir`. The line said `8, 12, 2` and itemised "2 in row 18".
   The Reviewer flagged the 8 and passed the 12; **the 12 was wrong too** — CPE-1913 replaced row 16's
   `File::create` with `claim_destination_handle` + `create_beneath`, so rows 2–14 own all eleven and row
   16 owns none. Both stale numbers are the two CPE-1913 changed, six lines above the rows this PR edits,
   in a ticket about enumerating rather than recalling. Rows 16 and 18's table cells were stale in the
   same way and were corrected with it. Verified identical on `origin/main`, so pre-existing.
2. **`make_dir_link` "panics rather than skipping" was imprecise.** `require_staged` panics only under
   `staging_is_strict()` (CI); locally it returns `false`. The effect holds because the fixture wraps the
   call in `assert!`, which is what the sentence now says — the panic-vs-`false` distinction is
   CI-vs-local.
3. **The F-B hole the Reviewer flagged does not exist, and the reason is worth recording.**
   "A leaf's mode cannot make anything else unwritable" is true across paths but was suspected false
   across entries sharing a name. Measured: `zip::ZipArchive` **collapses duplicate names in its central
   directory**, so a three-entry archive with `x.txt` twice lists two entries and extracts `done: 2` with
   the last copy winning — identically with a `0o444` first copy and with a `0o644` one, and identically
   on `origin/main`. `ZipWriter::start_file` refuses duplicates outright
   (`InvalidArchive("Duplicate filename")`), so the fixtures were hand-built STORED zips. A read-only
   leaf followed by *different* entries is unaffected (`done: 3`). Re-extraction into the same folder
   does fail on the second run with `os error 13` — and fails **identically on `origin/main`**, measured,
   so it is not this change's. The sentence was replaced by the measurement rather than narrowed on
   reasoning.

### Gates (round 2)

| Gate | Result | Delta |
|---|---|---|
| `cargo test --lib`, Windows | **2429 passed / 0 failed / 11 ignored** | +1 vs 2428 (the second new test is `#[cfg(unix)]`) |
| `cargo test --lib`, Linux/ext4 | **2415 passed / 2 failed / 11 ignored** | +2 vs the Auditor's 2413/2; the same 2 failures, both artifacts of the partial `crates/server`-only stage (they read repo files outside the crate) |
| `cargo clippy --all-targets -D warnings`, Windows, default + `specta` | clean | — |
| `cargo clippy --all-targets -D warnings`, Linux | clean | — |

Docs: `src/docs/explorer-archives.md` gained the shortcut-on-the-way-to-a-link bullet, in plain
language, including that the old behaviour **deleted** a same-named file of the user's.

Rebased onto `origin/main` (was 10 behind); no file in that delta overlaps this change.
