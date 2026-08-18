---
id: CPE-1777
title: Directory-creation errors drop the path at twenty-three more sites, two of them user-facing
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-18
closed:
---

## Problem

Found by the PR #930 (CPE-1729) review, which swept the tree rather than accepting the two sibling sites
the ticket had reported.

CPE-1729 fixed `split_file`'s `create_dir_all(out_dir).map_err(|e| e.to_string())` — a conversion that
discards the path entirely and passes the OS's raw wording through unexamined. A dangling link at the
output folder reported:

```
Cannot create a file when that file already exists. (os error 183)
```

which names neither where nor what, and calls a *directory* problem a "file". It is fixed at that one site
via `out_dir_error()`, following the convention CPE-1744 already set in `archive.rs`
(`extraction_dest_error`).

The measured sweep for `create_dir_all(...).map_err(...)`:

| Category | Count |
|---|---|
| Already include the path | 14 |
| Already fixed (`extraction_dest_error`, CPE-1744) | 4 |
| Fixed by CPE-1729 | 1 |
| **Pathless, remaining** | **25** |

## The 25, split by who chose the path

**Two take a path the user chose — these are the ones that matter:**

- `crates/server/src/backup.rs:115` — `create_dir_all(parent).map_err(|e| e.to_string())`
- `crates/server/src/batch_execute.rs:457` — `"could not create output dir: {e}"`. Note this one *looks*
  labelled but **never interpolates the actual path**, so it reads as informative while telling the user
  nothing they can act on. That is worse than the raw OS text, which at least does not pretend.

**Five in `archive.rs`** — explicitly set aside by CPE-1729's author as a different, confined-destination
risk class. Left as they are unless this ticket decides otherwise; say which.

**Eighteen build from `ctx.app_config_dir()` or fixed app-internal paths** — settings, macros, tags, the
index cache, checkpoint and metrics journals, ticket-board moves, catalog staging, admitted hosts:
`audit_journal.rs` (×2), `checkpoint_store.rs` (×2), `column_config.rs`, `content_index.rs`,
`folder_template.rs:224`, `index_service.rs` (×2), `macro_store.rs`, `metrics_journal.rs`,
`replay_baseline.rs`, `settings.rs`, `snapshot_schedule.rs`, `tags.rs`, `tray_quick.rs`, and
`src-tauri/src/lib.rs` (×3).

The user never typed those paths, so a dropped path costs less — but it does not cost nothing. When
settings or the index cache fails to initialise, the person debugging it is a developer or a support case,
and "permission denied" with no path is exactly as unhelpful to them as it was to the user in CPE-1729.

## What to do

- Fix the two user-facing sites first; they are the ones with a person on the other end.
- Then decide, and record, whether the eighteen app-internal ones are worth a sweep. A shared helper is
  cheap; the argument against is churn. Either answer is fine — an unrecorded answer is not, because that
  is how the next sweep re-derives the same list.
- Prefer reusing the existing convention (`extraction_dest_error` / `out_dir_error`) over a third
  spelling. Three implementations of one idea is how `deny_stat_of` needed the same fix three times.
- Test the **message**, not the failure. Assert it contains the path and names the cause. Asserting only
  that a `Result` is `Err` proves nothing about what this ticket is for.

## Worth folding in: one classification soft spot

The review found `out_dir_error`'s fallback is honest — an unclassifiable cause gets the path plus the raw
OS text, and it never forces a "link" or "file" label onto something it did not confirm. One exception
noted: a **read-only filesystem** may map to `ErrorKind::PermissionDenied` depending on OS and Rust
version, so it would report "permission denied" for what is really EROFS. That is a mislabel of degree
rather than of kind — the advice ("you cannot write here") stays directionally right — but if this ticket
touches that helper, it is worth distinguishing.

## Acceptance criteria

- [ ] `backup.rs:115` and `batch_execute.rs:457` name the path and the cause, via the existing helper
      convention rather than a third implementation.
- [ ] Breaking each reds a **distinct** test asserting on the message text, naming the path.
- [ ] A decision on the eighteen app-internal sites is recorded — swept, or deliberately left, with the
      reason.
- [ ] A decision on the five `archive.rs` sites is recorded likewise.
- [ ] No unclassifiable cause is forced into a specific label; the fallback names the path and passes the
      OS text through.

## Notes

Found by the Reviewer on **PR #930 / CPE-1729**, 2026-08-18, during the batched sprint. Related: CPE-1729
(the site just fixed), CPE-1744 (`extraction_dest_error`, the convention), CPE-1762 (a misleading error
that cost an entire release — the argument for why this class is worth closing).
