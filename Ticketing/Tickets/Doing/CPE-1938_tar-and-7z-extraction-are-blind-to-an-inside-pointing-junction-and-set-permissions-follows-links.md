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
| rows 15/16/23 — the three ZIP loops | already handle-gated (CPE-1913) | F-B fixed; component walk not needed |
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
