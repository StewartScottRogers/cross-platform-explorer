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

---

## Work Log — round 2 (reviewer CHANGES REQUESTED / SEC FINDINGS on PR #840)

The independent reviewer + security auditor confirmed both original escapes (symlink chain, contended
hard link) are genuinely closed, clippy clean in all three CI feature combos, 1930 tests green, perf
budget held, no new dependency and no bindings drift — and found one **regression this PR introduced**
plus one latent fail-open. Both are fixed here.

### F1 (BLOCKER, a regression) — the probe stopped addressing the files the writer does

`probe_no_follow` and `identity_following_links` called `CreateFileW` with the **raw** path
(`path.as_os_str().encode_wide()`). Without a `longPathAware` manifest that call is capped at
`MAX_PATH` (260). But every write in this crate goes through `std::fs`, which applies `maybe_verbatim`
and therefore *does* reach longer paths. Round 1 replaced a verbatim-aware `std::fs::symlink_metadata`
probe with a raw Win32 one, so the probe and the writer stopped addressing the same set of files.

`CreateFileW` failed with `ERROR_PATH_NOT_FOUND` (3) → `io::ErrorKind::NotFound` →
`classify_open_failure` → `Probe::Absent` → `Containment::Inside`. **It failed OPEN**, and made this
case strictly worse than `main`: on base commit `42dfcdcd` the identical scenario was refused.

Fixed in two layers:

1. **`verbatim_wide` (`batch_media.rs`, new)** — a `std::sys::path::windows::maybe_verbatim`
   equivalent, used by BOTH Windows probes. Because `\\?\` disables *all* kernel path normalisation, it
   only ever prefixes a path that `GetFullPathNameW` has already made fully-qualified, `..`-free and
   back-slash-separated, then picks the prefix by shape exactly as `std` does: `C:\…` ⇒ `\\?\C:\…`,
   `\\.\…` ⇒ `\\?\…`, `\\server\share` ⇒ `\\?\UNC\server\share`, and an already-verbatim (`\\?\`) or NT
   (`\??\`) path is returned untouched. `GetFullPathNameW` comes from the `Win32_Storage_FileSystem`
   feature already enabled — **no new dependency**.
   *Cost:* a path that is already verbatim, or shorter than `std`'s 248 `LEGACY_MAX_PATH` and not UNC,
   returns after two slice comparisons and a length test — no syscall and no allocation beyond the
   `Vec<u16>` the raw encoding already needed. The per-entry directory census pays nothing.
2. **Belt and braces in `classify_open_failure`** — at or past `MAX_PATH`, `ERROR_PATH_NOT_FOUND` /
   `ERROR_INVALID_NAME` / `ERROR_BAD_PATHNAME` / `ERROR_FILENAME_EXCED_RANGE` are exactly what a
   *truncation* looks like, so they now classify `Unreadable` (which both callers refuse on) instead of
   `Absent`. `ERROR_FILE_NOT_FOUND` — the ordinary "this output doesn't exist yet" answer, and the
   overwhelmingly common one — is deliberately NOT in that set, so the common path costs nothing.

`follow_link_chain` is unchanged on purpose: it keeps building `parent().join(target)` with the `..`
segment intact and lets path resolution collapse it (`GetFullPathNameW` for long paths, the kernel for
short ones), rather than collapsing lexically here — lexical `..` collapsing is wrong when an
intermediate component is itself a link.

### F2 (medium) — a degenerate identity was accepted as a valid one

`GetFileInformationByHandle` is documented as not supplying a usable file index on several network
redirectors: it *succeeds* and returns zero. Every object on such a volume then carries the same
`(volume, index)`, so the landing-dir-vs-selected-dir comparison in `resolve_output_containment` and
the census lookup in `real_target_containment` would judge any two unrelated directories "the same
place" — a fail-open invisible from the call site, because the API reported success.

`FileIdentity::is_degenerate` now rejects a zero in either half (zero is not a legal value on either
platform: `ino`/`st_dev` 0 denote no file / no device on Unix; a zero file index or volume serial is
Windows' "not supported here"). It is applied through two pure helpers, `facts_or_unreadable` (⇒
`Probe::Unreadable`) and `identity_or_none` (⇒ `None`, which `dir_identity`'s callers already treat as
a containment failure), on **both** platforms. Pure by design so the volume CI has no access to can be
reproduced by injecting the degenerate value.

### F5 — one weak assertion tightened

`cpe_1642_hard_link_alias_is_still_refused_while_the_file_is_held_exclusively` asserted only
`assert_ne!(verdict, Inside)`, which would pass even if the identity probe were entirely broken and
answered `Unverifiable` for everything — so it did not actually prove the
`FILE_READ_ATTRIBUTES`-beats-`share_mode(0)` mechanism the contended-hard-link fix rests on. Now
`assert_eq!(verdict, Escapes)`, which is only reachable if the attribute open *succeeded* through the
exclusive hold, read a real link count of 2, and censused the folder. Paired with a new positive
control, `cpe_1642_contended_hard_links_wholly_inside_the_folder_are_still_allowed` — same exclusive
holder, both names inside the folder, must still be `Inside`.

### New coverage (7 tests; there was previously NO long-path or `\\?\` coverage at all)

| Test | File | Guards |
|------|------|--------|
| `cpe_1642_over_max_path_symlink_alias_is_refused_and_the_victim_bytes_are_untouched` | `batch_execute` | REV-G2 end-to-end through `execute_plan`, byte-verified |
| `cpe_1642_symlink_alias_past_max_path_is_a_proven_escape` | `batch_media` | the same mechanism at the containment-check level |
| `cpe_1642_ordinary_absent_output_past_max_path_is_still_allowed` | `batch_media` | no false refusal of ordinary deep-folder batches |
| `cpe_1642_verbatim_prefixed_paths_are_probed_not_re_prefixed` | `batch_media` | a `\\?\` path is neither mangled nor waved through (escape + positive control) |
| `cpe_1642_degenerate_identity_is_never_accepted_as_a_real_one` | `batch_media` | F2, by injection |
| `cpe_1642_a_real_directory_still_has_a_readable_identity` | `batch_media` | F2 guard doesn't reject real volumes |
| `cpe_1642_contended_hard_links_wholly_inside_the_folder_are_still_allowed` | `batch_media` | F5 positive control |

### Red-then-green, actually run

The production hunks were reverted in place (probes back to the raw `encode_wide` path, the length
guard removed, both degeneracy helpers made pass-through) with the new tests left in. **3 of the 7
failed**, in the way the reviewer described:

- `cpe_1642_over_max_path_…` — `left: [137, 80, 78, 71, …]` (a PNG) vs `right: [86, 73, 67, …]`
  ("VICTIM ORIGINAL CONTENT …"): the victim's bytes really were replaced.
- `cpe_1642_symlink_alias_past_max_path_is_a_proven_escape` — `left: Inside, right: Escapes`.
- `cpe_1642_degenerate_identity_is_never_accepted_as_a_real_one` — "a degenerate identity must probe as
  Unreadable …, not Real".

The four controls (`verbatim_prefixed`, `ordinary_absent_output_past_max_path`,
`contended_hard_links_wholly_inside`, `hard_link_alias_is_still_refused`) passed in both states, as they
must — they are false-refusal and non-vacuity guards, not the red case. Sources restored: all 7 green.

### Verification

- `cargo clippy --all-targets -- -D warnings` clean in all three CI feature combos: default,
  `--features index`, and `--features pdf-thumb,video-thumb,waveform,dicom-thumb`.
- Full `cargo test` in `crates/server`: **1937 passed / 0 failed / 2 ignored** (1930 baseline + the 7
  new tests), plus every integration target green.
- `cargo test --release --lib -- --ignored cpe_1623_plan_timing_for_2000_files`: **198.0 / 202.9 /
  210.9 / 215.5 / 229.4 ms** against the ticket's 209-219 ms baseline. (Interleaved control runs of the
  *unmodified* branch tip on the same machine spanned 194-423 ms, so this benchmark's noise band is far
  wider than any effect of the change — as expected, since `verbatim_wide`'s fast path adds two slice
  comparisons per probe and no syscall.)
- No new dependency (`GetFullPathNameW` is in the already-enabled `Win32_Storage_FileSystem`), no
  `Cargo.toml`/`Cargo.lock` change, no `specta::Type` struct touched — no bindings regen needed.

### Deliberately NOT fixed here (filed separately by the Foreman)

- **F3** — name-surrogate reparse tags `std` does not call symlinks (the probe identifies the stub, the
  write follows it).
- **F4** — the census perf cliff (`plan()` and `execute_plan_walk` build separate `ParentCache`s, so up
  to two full censuses).

### Reviewer round 3 — Foreman-applied fix (2026-08-11)

The round-2 re-review returned **SEC PASS** with one blocking item, and it was a good one. The reviewer
neutralised each defence layer separately:

- Layer 1 (`verbatim_wide` returning the raw path): suite went **RED** — the guard is load-bearing and
  covered.
- Layer 2 (the `classify_open_failure` length guard): suite stayed **fully GREEN**. Deleting the guard
  outright broke nothing.

That mattered because the same reviewer had just demonstrated layer 2 is what saves the victim's bytes
when layer 1 fails (it degrades the verdict to `Unverifiable`, which every caller refuses on). An
unpinned guard in a ticket whose entire history is "each round eroded a guard no test was holding" is
the exact failure mode to close.

Applied directly by the Foreman rather than spending another worker round-trip, since the fix was
exactly prescribed:

- Added `cpe_1642_classify_open_failure_length_guard_is_pinned` — a pure table test (no filesystem):
  the four truncation codes past `MAX_PATH` must be `Unreadable`; the same codes below it keep their
  ordinary meaning; `ERROR_FILE_NOT_FOUND` stays `Absent` at any length (the deliberate exclusion that
  keeps legitimate deep-folder batches working); the 259/260 boundary is pinned from both sides; an
  unrelated failure (`ERROR_ACCESS_DENIED`) is `Unreadable` regardless of length.
- Corrected the coverage claim on `cpe_1642_ordinary_absent_output_past_max_path_is_still_allowed`, which
  said it pinned the length guard. It does not — it reaches `classify_open_failure` through layer 1, where
  the guard is inert. The doc now says so and points at the new test.

Verified red-then-green: with `wide_len >= MAX_PATH` short-circuited to `false`, the new test fails with
*"os error 3 on a past-MAX_PATH path must fail CLOSED as Unreadable, never Absent"*; restored, it passes.
`cargo test --lib batch_media` 54 passed / 0 failed; `cargo clippy --all-targets -- -D warnings` clean.

Reviewer's other observations, deliberately not actioned here: the intentional short-UNC divergence from
`std` (harmless — analysed as bit-bucket-only), and `scan_dir_link_census` reporting `Escapes` for an
entry that vanished mid-scan where `incomplete = true` would be more honest. The latter is a
message-accuracy nit of the same family as CPE-1652 and belongs with it.
