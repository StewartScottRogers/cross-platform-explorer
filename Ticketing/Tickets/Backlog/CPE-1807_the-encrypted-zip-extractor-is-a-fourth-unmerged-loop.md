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
