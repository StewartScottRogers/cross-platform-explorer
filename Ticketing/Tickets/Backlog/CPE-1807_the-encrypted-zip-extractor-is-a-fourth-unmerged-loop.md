---
id: CPE-1807
title: the encrypted-zip extractor is a fourth unmerged zip loop, and a doc comment claims it does not exist
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

`extract_zip_encrypted` (`crates/server/src/archive.rs:1974`, loop at `2858`) is a **fourth zip extraction
loop** that was not folded into the shared path CPE-1759 consolidated. Two smaller wrongs travel with it:

- A doc comment asserts the consolidated loop "is now the only zip extractor" — **false**, and this repo
  has now shipped a factually wrong comment in this file twice, both times by restating it from memory of
  its shape rather than re-checking.
- A broken intra-doc link at `archive.rs:852` points at `zip_entry_out`, deleted by CPE-1773/1774/1775.

## Why it matters

Every guard the archive cluster added — path traversal, symlink escape, entry-name refusal, the
refusals-skip-failures-abort rule from CPE-1759 — was reasoned about against the loops that were merged.
An unmerged fourth loop is a path where those guarantees have to be **verified separately**, and nobody has
stated whether they hold there.

The wrong comment makes that worse: the next person to reason about this file will be told the fourth loop
does not exist.

## What to do

- **Audit `extract_zip_encrypted` against each guard the shared loop enforces** before deciding whether to
  merge it. Enumerate them; for each, say whether it holds, and how you established that. If a guard is
  missing, that is a security finding and outranks the consolidation.
- Then decide whether to fold it into the shared loop or keep it separate — encryption may justify a
  separate path. **Say why**; do not merge for tidiness alone.
- Fix the comment and the intra-doc link. Per the standing instruction already in this file: **re-grep
  before editing that paragraph** rather than restating it.

## Notes

Filed by the Foreman from the independent review of PR #958, 2026-08-20.

Related: **CPE-1759**, **CPE-1773/1774/1775**, **CPE-1786**.

## Work Log

### 2026-08-20 — merged, audited, docs fixed

**Audit (against `extract_zip_archive_stream`'s guards), before merging:**
- `entry_name_is_safe` (zip-slip / reserved-name refusal) — already present in the old loop, identical.
- `entry_dir_action`/`entry_sink_action` (leaf-link + per-component containment) — already present,
  identical order (asked before `create_dir_all(parent)`).
- Symlink-entry containment (`link_target_action`, via `entry.is_symlink()`) — **missing** in the old
  one-shot loop. A zip entry's symlink flag and declared target are central-directory metadata, which the
  `zip` crate does not AES-encrypt (only the entry's content stream is), so this guard reads correctly on
  a password-protected entry without decrypting anything — the streamed encrypted extractor
  (`extract_zip_encrypted_streamed`) already proved this by already calling the shared loop. No finding:
  merging *adds* a guard, drops none.
- Unix permission-bit restoration (`unix_mode()`) — same reasoning, also metadata, also newly gained.
- No guard the old loop had is dropped by the merge, and the password itself is threaded unchanged:
  `archive.by_index_decrypt(i, password.as_bytes())` is now called from inside the shared loop instead of
  the old loop's own body, at the same point in the per-entry sequence.

**Decision: merge.** No guard justified keeping encryption on a separate path — decryption only changes
*which bytes* `entry`'s reader yields, not any of the metadata the guards inspect. `extract_zip_encrypted`
now delegates to `extract_zip_archive_stream(&mut archive, dest_path, Some(password), &never, &mut |_| {})`,
the same pattern `extract_archive`'s zip branch (row 23) already used for the plain path. The skip stays
**silent** on this path (unchanged `Result<String, String>` signature — no `ArchiveReport` to put a note
in), same limitation row 23 already carries.

**Docs fixed:** the broken `[`zip_entry_out`]` intra-doc link (that pre-pass function was deleted by
CPE-1773/1774/1775) now points at the current zip-side caller of `link_target_action`
(`extract_zip_archive_stream`). `extract_zip_archive_stream`'s own doc, which as of CPE-1814 correctly
said `extract_zip_encrypted` was "a fourth, unmerged loop", now says there is no fourth loop left. Updated
the CPE-1733 guard table's rows 15/17/18 and its `create_dir_all`/`File::create` count reconciliation
paragraph, which the merge changed (8 `create_dir_all` calls, down from 11; 12 `File::create` calls, down
from 13 — both re-verified against the source with `grep`, not assumed) — a stale count is exactly the
kind of restated-from-memory claim this ticket exists to stop.

**Scope held:** did not touch CPE-1808/1809/1812/1829 — no code in their territory was changed.

**Red-proof (no tests added/changed — the existing suite already exercises this function; verified it
would still catch a broken merge):** changed `Some(password)` → `None` in the new delegating call at
`extract_zip_encrypted` (one word). Result: 6 tests failed immediately with `"unsupported Zip archive:
Password required to decrypt file"` — `encrypted_zip_round_trips_and_rejects_a_wrong_password`,
`compress_to_zip_encrypted_streamed_round_trips_and_rejects_a_wrong_password`,
`row15_extract_zip_encrypted_skips_an_entry_that_lands_on_a_link`,
`rows_15_and_16_refuse_a_live_link_and_still_extract_the_rest`,
`rows_15_to_20_refuse_a_file_entry_addressed_through_a_symlinked_intermediate_directory`,
`row18_refuses_a_directory_entry_that_would_be_created_outside_the_extraction_folder`. Reverted; all green
again.

**Gates:** `cargo clippy --manifest-path crates/server/Cargo.toml --all-targets -- -D warnings` — clean,
no warnings. `cargo test --manifest-path crates/server/Cargo.toml --lib` — `2272 passed; 0 failed; 4
ignored`.

PR: see branch `cpe-1807-encrypted-zip-loop`.

### 2026-08-20 — Reviewer's CHANGES REQUESTED on PR #975, addressed

The Reviewer verified the code itself could not break the guard (metadata-reads-before-decryption claim
confirmed against `zip-2.4.2` source AND measured on real AES-256 archives; nothing lost, diffed step by
step; six escape attempts all refused) — but found three doc/test blockers, all in the record rather than
the code:

1. **Count paragraph's own subtraction was wrong.** Wrote "which removed two"; the breakdown beneath it
   (row 17 -1, row 18 -2) sums to three, and `11 - 2 = 9 ≠ 8`. Fixed to "removed three — one off row 17,
   two off row 18".
2. **The security-delta bullet was framed backwards.** It read as "symlink containment newly gained" with
   no context, which a reader takes as "the old path had a link-escape hole." It did not — the old loop
   pushed a symlink entry through `File::create`/`io::copy` like any other entry, so it could not create a
   real link AT ALL, benign or escaping (one of `link_target_action`'s own doc's three SAFE policies for
   CPE-1774). Rewrote the bullet: the merge is what makes this path *able* to create a real symlink for
   the first time, and `link_target_action` is what keeps that new ability from being escaped. Added a
   line documenting the user-visible consequence, filed as CPE-1837: an entry
   that used to appear as a readable (if wrong-looking) text file can now vanish with no note on refusal,
   since this signature still has nowhere to put one.
3. **Nothing pinned the merge.** Added `cpe1807_encrypted_zip_symlink_entry_whose_target_escapes_creates_no_link`,
   the encrypted leg the CPE-1774 zip table was missing, using a new `craft_zip_with_symlink_encrypted`
   helper (real AES-256 entries, all four CPE-1774 escaping-target shapes). **First draft of this test was
   itself too weak** — it asserted "not a symlink" + "content != victim bytes", which the OLD duplicated
   loop's actual failure mode (writes the target STRING as an ordinary file's content) also satisfies, so
   it passed green even with the deleted loop restored verbatim. Rewrote the primary assertion to
   `!leaf.exists()`, the one thing that actually differs between skip (merged) and text-file (regressed).
   Red-proofed by restoring the deleted loop into `extract_zip_encrypted` and re-running just this test:
   failed on the first target shape (`plain-parent`) with the new assertion's message, confirming it
   catches the regression; reverted, confirmed green again, then re-ran clippy and the full suite.

**Follow-ups mentioned per the Foreman's instruction, not fixed here:**
4. This round's doc rewrite adds `public documentation for X links to private item` rustdoc warnings.
   Corrected figures per the Reviewer's re-measurement on `84b77849` (the first pass through this Work Log
   had the crate total wrong): 505 total warnings in the crate, 21 in `archive.rs`, of which 20 are the
   `links to private item` class. Net delta from this PR versus main: +7 (main 498 → 505 crate-wide;
   `archive.rs` 14 → 21; `extract_zip_encrypted`'s own doc comment alone went from 1 to 9). **Platform
   note:** a clean `cargo doc --manifest-path crates/server/Cargo.toml --no-deps --lib` on this Windows
   dev box reproducibly shows fewer (487 total, 20 in `archive.rs`, matching the Reviewer's `links to
   private item` subset exactly) — plausibly `#[cfg(unix)]`-gated doc content that only compiles, and so
   only gets doc-linted, on a unix target; not independently resolved this round. No CI gate runs
   `cargo doc` either way, and the module already carries this warning class throughout, so this is
   style-consistent noise, not a regression — but it is real, not zero, and both platforms' counts are
   now on the record rather than one unreproduced figure.
5. On unix, the shared loop's deferred `set_permissions` pass can now turn a fully-written encrypted
   extraction into an `Err` after every file has landed, which the old one-shot loop could not do (it had
   no mode-restoration pass at all). Same shape as rows 16/23 already have, so not a regression relative to
   its siblings, but new for this specific function.

**Gates, re-run after all three blocker fixes:** `cargo clippy --manifest-path crates/server/Cargo.toml
--all-targets -- -D warnings` — clean, no warnings. `cargo test --manifest-path crates/server/Cargo.toml
--lib` — `2273 passed; 0 failed; 4 ignored` (2272 + the 1 new test).


### 2026-08-20 — PR #975 APPROVED; three record fixes before merge

The Reviewer independently reproduced the trap in the first draft of `cpe1807_encrypted_zip_symlink_entry_whose_target_escapes_creates_no_link`
and went further: it patched the assertion into a non-fatal probe and measured all four escaping-target
shapes against the re-duplicated loop, not just the first. Every one came back `is_symlink=false` with
content that is neither a link nor the victim's bytes (`plain-parent` -> `..\victim.txt`, `absolute` ->
the full outside path, `dot-chain` -> `x/../../victim.txt`, `mixed-separators` -> `..//..\victim.txt`),
confirming `!leaf.exists()` really is the only assertion that discriminates skip from regression on any
of them, not just the one this session's own red-proof happened to trip over first (the run aborts at the
first failing assertion, so the code comment saying "failed on all four shapes" and the Work Log above
saying "failed on plain-parent" are both true at once -- first-to-fail versus all-independently-trigger).

Three more record fixes required before merge, all doc/Work-Log, no further code or test changes:

1. **A doc comment claimed a ticket that did not exist.** `archive.rs:2428` said the silent-vanish
   consequence was "filed as a separate ticket" before it had actually been filed. It is filed now,
   **CPE-1837**, and the doc comment plus this Work Log (above) now name it directly rather than saying
   "filed separately."
2. **Corrected the rustdoc warning counts above** (see the updated point 4): crate total is 505, not 487;
   `archive.rs` carries 21 warnings, 20 of the `links to private item` class; net delta from this PR is
   +7 versus main (498 -> 505 crate-wide, 14 -> 21 in `archive.rs`, 1 -> 9 on `extract_zip_encrypted`'s own
   doc comment alone).
3. **Added the silent-vanish consequence and follow-up 5 (unix `set_permissions` on an encrypted
   extraction) to the PR body**, not just this ticket -- they are the two user-visible/behaviour-relevant
   facts a reviewer deciding whether to merge would want on the page, not buried in the ticket file.

**Gates, re-run after these three fixes:** `cargo clippy --manifest-path crates/server/Cargo.toml
--all-targets -- -D warnings` -- clean, no warnings. `cargo test --manifest-path crates/server/Cargo.toml
--lib` -- `2273 passed; 0 failed; 4 ignored` (unchanged from the previous round: only doc/Work-Log text moved, no code or test bodies touched).

Note carried forward, not acted on: `cpe1807_encrypted_zip_symlink_entry_whose_target_escapes_creates_no_link`
is not platform-gated, so CI's Linux/macOS legs exercise real symlink creation that this Windows dev box
only partly can; if CI reds, look there first.
