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
