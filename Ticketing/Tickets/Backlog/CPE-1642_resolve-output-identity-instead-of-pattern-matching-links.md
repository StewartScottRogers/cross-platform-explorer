---
id: CPE-1642
title: "Batch Media containment should resolve the output's true file identity, not pattern-match link shapes — symlink chains and a contended hard-link read still escape"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready, big-design]
created: 2026-08-11
closed:
---

## Why
Three rounds of security audit on CPE-1623 (PR #828) each closed a real escape and each turned up a new
variant of the same underlying problem. The fixes shipped are genuine and verified — but the *approach* has
reached its limit, and this ticket exists to replace it rather than extend it a fourth time.

**What CPE-1623 did fix and verify (do not redo):** the original `..\..\folder\name` traversal from the
rename box; the IPC bypass where `execute_plan_walk` accepted hand-built `PlannedItem`s without re-deriving
containment; `C:foo` on bare-filename inputs; extensionless-input `..`; whole-segment `..` handling (with
`shot..final` correctly accepted); `Convert.to_ext` validation; and **single-hop** symlink/junction and
hard-link aliasing of the output's final component. Each was byte-verified with a reproduced negative
control. An intermediate junction in the path is also correctly resolved (independently confirmed).

## What still escapes

**A — same-directory symlink chain.** `link_alias_escapes` reads exactly **one hop** (`read_link` on the
output) and compares only that target's *directory* against the input's. If the immediate target is itself a
symlink sitting (textually) in the same folder, the directory comparison passes and nothing asks whether
*that* name is also a link pointing further out.

Demonstrated end-to-end with real bytes: `linkA → linkB` (relative, same dir), `linkB → outside\important.jpg`.
A `PlannedItem{input: selected\photo.jpg, output: selected\linkA.jpg}` with `confirmed_overwrite: true` gave
`Ok(BatchReport{written:1})`, and the outside victim's bytes changed.

**B — hard-link count read fails open under contention.** `hard_link_count`'s Windows path defaults to `1`
whenever `CreateFileW` fails — the same fallback used for the benign not-yet-existing case. So a genuinely
multiply-linked file whose open merely fails is treated as not-linked.

Demonstrated without elevation: with `selected\link.jpg` hard-linked to `outside\important.jpg` (correctly
refused when uncontended), holding an exclusive handle from an ordinary process
(`OpenOptions::share_mode(0)`) made `output_escapes_input_dir` return `false`. Any concurrent holder — another
process, an AV scanner, even a second thread in the same batch — flips the fail-closed rule to fail-open.

## Why this needs a different approach, not a fourth patch
Each round has fixed the shape that was demonstrated and left the next shape open: raw text → one-hop links →
chains → contended reads. The check is **pattern-matching link shapes**, and the space of shapes is not
bounded by our imagination.

**The durable fix is to resolve the output's true identity once, and compare identities** — not paths, and not
link-shape heuristics. Concretely worth designing around:
- Resolve the output (and its parent) to a real filesystem identity — on Windows, the volume serial + file
  index via an opened handle; on Unix, `(dev, ino)` — and ask whether that identity lives under the selected
  directory's identity. This collapses chains, hard links, junctions and future shapes into one question.
- Prefer **resolve-then-write on the same handle** where possible, which also narrows the TOCTOU window
  CPE-1624 tracks — the two tickets may share a design.
- Every failure to establish identity must **fail closed**, including a failed open under contention. That is
  precisely finding B: the fallback must be "refuse", never "assume unlinked".
- Keep it O(n)-amortized and memoized. `plan()` is ~209-219ms for 2000 files today (from 12 minutes before
  CPE-1613); there are canonicalize-count guards, and one was verified non-vacuous by injecting a real O(n²)
  regression and watching it fail. Do not regress either property.

## Also fix here (small, found in the same audit)
The refusal message for the accepted hard-link false positive reads *"...would land outside its own input's
folder"*, which is **factually untrue** in that case — nothing left the folder; the check merely couldn't
prove it hadn't. Say what is actually true ("couldn't verify this stays inside the folder"), since telling a
user something false about why their operation failed is its own defect.

## Acceptance criteria
- Findings A and B are refused, byte-verified, with negative controls that fail against today's code.
- Every identity-resolution failure path is enumerated in the work log with its direction; all fail closed.
- The single-hop cases CPE-1623 fixed still hold — re-run its regression tests, don't just assume.
- No new false positives on ordinary batches; the hard-link "all names inside the folder" case ideally stops
  being a false positive once identity is resolved properly.
- `plan()` stays linear and within ~10% of today's timing; the canonicalize guards stay green and non-vacuous.
- Refusal messages state what is actually true.

## Risk framing (why CPE-1623 merged with this open)
Both remaining escapes require the attacker to **already be able to create files inside the folder the user
selected**. On `main` before CPE-1623, the far easier `..\..\x` attack needed no such foothold and destroyed
an arbitrary file with no confirmation. So CPE-1623 is a strict, large reduction in attack surface, and
holding it back to chase a fourth variant would have left the easy attack shipping. This ticket carries the
remainder.

**Conflict surface:** `crates/server/src/batch_media.rs` (`output_escapes_input_dir`, `link_alias_escapes`,
`hard_link_count`, `path_key`), `crates/server/src/batch_execute.rs`. Overlaps **CPE-1624** (TOCTOU per-write
re-check + ADS) — these two should probably be designed together.

## Work Log

**2026-08-11 — implemented (branch `cpe-1642-output-identity`).**

### What changed
`link_alias_escapes` + `hard_link_count` (link-shape pattern-matching) are **deleted** and replaced by
identity resolution in `crates/server/src/batch_media.rs`:

- `FileIdentity { volume, index }` — `(dwVolumeSerialNumber, nFileIndexHigh:nFileIndexLow)` from
  `CreateFileW` + `GetFileInformationByHandle` on Windows; `(dev, ino)` from `MetadataExt` on Unix. No new
  dependency: the `windows` crate (0.56, `Win32_Storage_FileSystem`) was already vendored for the old
  hard-link count, and the Unix side reads off the `symlink_metadata` call the module already made.
- `probe_no_follow(path) -> Probe` — `Absent` / `Link` / `Real(FileFacts{id, links, is_dir})` /
  `Unreadable`. On Windows this is ONE handle open (replacing the old `symlink_metadata` + separate
  count open, so the common path got *cheaper*, not costlier); on Unix it is the same single stat as
  before with `dev`/`ino`/`nlink` read off it.
- `follow_link_chain` — walks the **whole** symlink/junction chain (relative targets resolved against
  their own link's parent, `read_link` not `canonicalize` so a *dangling* end still resolves), bounded at
  40 hops, refusing on an unreadable link, a self-referential hop, or the bound. Finding A is dead.
- `real_target_containment` — a multiply-linked target is settled by `scan_dir_link_census`: a
  **bounded, single-directory, memoized** census of the selected folder by identity. All names accounted
  for inside ⇒ allowed; fewer names than `nlink` ⇒ a name provably exists outside ⇒ refused. This also
  retires CPE-1623's documented false positive (hard links wholly inside the folder are now allowed),
  without the full-volume walk that ticket rightly rejected.
- `Containment { Inside, Escapes, Unverifiable(&'static str) }` + `classify_output_containment` is now the
  one production entry point (`output_escapes_input_dir` survives only as a `#[cfg(test)]` boolean
  wrapper). `plan()` and `execute_plan_walk` render the two refusals **separately**, so the message for an
  unverifiable output no longer claims it "would land outside its own folder" — the untrue-message defect
  in the ticket.

### Finding B specifically (the fail-open)
Two things fixed it: the identity open now asks for **`FILE_READ_ATTRIBUTES`, not `GENERIC_READ`**
(Windows' share-mode conflict check ignores attribute-read rights, so it succeeds against a file another
process holds with `share_mode(0)`), and a failed open is **classified by its real error** instead of
defaulting to "1 link" — only codes that mean "nothing is there" (`NotFound`, `ERROR_INVALID_NAME`,
`ERROR_BAD_PATHNAME`, `ERROR_DIRECTORY`) become `Absent`; sharing violations, access denied and anything
unclassified become `Unreadable`, which every caller refuses on.

### Every identity-resolution failure path, and its direction (all fail CLOSED)
| Failure | Verdict |
|---|---|
| `probe_no_follow` open/stat fails for a reason other than "nothing is there" (sharing violation, access denied, unclassified) | `Unverifiable` → refuse |
| `GetFileInformationByHandle` fails on an opened handle | `Unverifiable` → refuse |
| `read_link` fails on a path whose link bit is set | `Unverifiable` → refuse |
| Link chain is self-referential / cyclic / exceeds 40 hops | `Unverifiable` → refuse |
| Chain terminal has no parent component | `Unverifiable` → refuse |
| Landing directory's identity unreadable, or the selected folder's identity unreadable | `Unverifiable` → refuse |
| Chain "ends" on something still reported as a link | `Unverifiable` → refuse |
| `read_dir` of the selected folder fails (census needed) | `Unverifiable` → refuse |
| Census incomplete (an entry's type/identity unreadable) **and** names found < `nlink` | `Unverifiable` → refuse |
| Platform is neither Windows nor Unix and anything exists at the output | `Unverifiable` → refuse |

The only `Inside` answers are: nothing exists at the output; a real single-linked file/dir in the selected
folder; a chain that lands (live or dangling) inside the selected folder; or a multiply-linked file whose
every name was *counted* inside the selected folder.

### Test evidence
New tests (9), all passing, none skipped on this machine (`--nocapture` shows no SKIP lines; symlink tests
print a loud `SKIPPING … verified NOTHING` and return if Developer Mode is unavailable — they never pass
silently):

- `batch_media`: `cpe_1642_two_hop_symlink_chain_escapes`,
  `cpe_1642_symlink_chain_landing_back_inside_the_folder_is_allowed` (negative control),
  `cpe_1642_symlink_cycle_is_refused_as_unverifiable_and_terminates`,
  `cpe_1642_hard_link_alias_is_still_refused_while_the_file_is_held_exclusively` (Windows),
  `cpe_1642_hard_links_wholly_inside_the_selected_folder_are_allowed` (negative control — the retired
  false positive), `cpe_1642_hard_link_with_a_name_outside_the_folder_is_a_proven_escape`.
- `batch_execute` (byte-proven, full `execute_plan` runs):
  `cpe_1642_symlink_chain_alias_is_refused_and_the_victim_bytes_are_untouched`,
  `cpe_1642_contended_hard_link_alias_is_refused_and_the_victim_bytes_are_untouched` (Windows),
  `cpe_1642_unverifiable_output_is_refused_without_claiming_it_left_the_folder` (message accuracy).

**Negative control, actually run:** the fixed sources were set aside, `git checkout` restored pre-fix
`batch_media.rs`/`batch_execute.rs`, and equivalent control tests were run against them —
`cpe_1642_control_two_hop_symlink_chain_escapes` and `cpe_1642_control_contended_hard_link_escapes` **both
FAILED** ("CONTROL: chain must be refused" / "CONTROL: contended hard link must be refused"), confirming
both escapes are real on this machine and that the new tests are not vacuous. The fixed sources were then
restored and re-verified.

**Regression + perf:** full `cargo test` in `crates/server` green (1930 lib tests + all integration
targets, 0 failed), including every CPE-1623 containment test re-run unchanged. Both canonicalize-count
guards (`plan()`'s and `execute_plan_walk`'s) still pass — identity probes add no `canonicalize` calls.
`cargo test --release -- --ignored cpe_1623_plan_timing_for_2000_files` measured **217.9 ms**, inside the
ticket's 209-219 ms baseline. `cargo clippy --all-targets -- -D warnings` clean in **both** feature modes
(default and `--all-features`). No TS touched, no `specta::Type` struct touched — no bindings regen needed.

### Assumptions
- Windows file identity is `(volume serial, 64-bit file index)`. That is stable on NTFS/ReFS local volumes
  and on SMB shares that report it; a filesystem that reports a constant or recycled file index could in
  principle equate two distinct files. Unix `(dev, ino)` has the same theoretical caveat. Judged correct
  for the platforms the app ships on, and strictly stronger than the path-string matching it replaces.
- A reparse point that `std` does not classify as a symlink (cloud placeholder, dedup stub) is treated as
  the ordinary file it is, not as a link — otherwise every batch in a OneDrive-backed folder would be
  refused.
- An existing **directory** at the output path is still `Inside` (unchanged behaviour): the write fails on
  its own later, and it aliases nothing.

### CPE-1624 seam (deliberately left, not implemented)
`classify_output_containment` is a pure function of `(input, output, cache)`, and the cache memoizes
**only directory-level** facts — never an individual output's probe. Calling it again immediately before
each write is therefore a genuine re-resolution, not a replay of the plan-time answer; CPE-1624 can add
that call in `execute_one`/`execute_plan_walk` without touching this logic. ADS/colon handling is
untouched and remains 1624's.
