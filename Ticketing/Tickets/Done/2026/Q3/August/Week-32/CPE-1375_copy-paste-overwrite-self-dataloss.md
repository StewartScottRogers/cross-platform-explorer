---
id: CPE-1375
title: "CRITICAL: copy → paste-in-place → Replace permanently deletes the file/folder"
type: Bug
status: Done
priority: Critical
component: Backend
tags: [ready]
epic: CPE-059
created: 2026-08-06
closed: 2026-08-06
---

## Problem (found by the file-operations adversarial audit) — UNRECOVERABLE DATA LOSS

Copy an item, paste it back into the folder it already lives in, then choose **Replace** in the conflict
dialog → the item is **permanently deleted** and nothing is written in its place. Hard delete (not the
Recycle Bin), so it's unrecoverable. For a folder it's `remove_dir_all` — the **entire tree** is destroyed.

Repro: folder X contains `a.txt`; select it, Ctrl+C; still in X, Ctrl+V → conflict dialog → **Replace** →
`a.txt` is gone. The toast reads "Copied 1, 1 failed" — the user is told something failed, not that their
original was destroyed. Directory variant deletes the whole subtree.

## Root cause

`run_transfer` (`src-tauri/src/lib.rs`) computes `target = dest_dir.join(name)` where `name` is the
SOURCE's own filename. When the paste destination is the source's own parent, `target == src`. With policy
`Overwrite`, `resolve_conflict` does `remove_file`/`remove_dir_all(base_target)` (== the source) BEFORE
copying — destroying the original — then `copy_tree_streamed(src, target)` opens the now-missing source and
fails. No `src == target` guard existed. Only reachable via the paste conflict dialog's "Replace"
(all other copy entry points hardcode keepboth; the sync do_copy/move_into always unique_target-rename).

## Fix

Added `same_path(a, b)` (canonicalize + literal-compare fallback) and, in `run_transfer` before
`resolve_conflict`, an **Overwrite-scoped** self-target skip:
`if policy == ConflictPolicy::Overwrite && same_path(&base_target, src) { skip }`. Overwriting an item with
itself is a no-op. Scoped to Overwrite so `Keepboth` still falls through to `unique_target` and produces the
legitimate in-place duplicate ("a - Copy.txt"); `Skip` already no-ops.

## Tests + review

`run_transfer_skips_a_copy_onto_itself_instead_of_destroying_it`: asserts a file AND a folder-tree survive
an Overwrite-onto-self, AND that Keepboth-onto-self still creates "a - Copy.txt" with the original intact.
5/5 transfer tests pass. Independently reviewed: the data-loss hole is **airtight** — verified no false-
negative (when target==src and exists, both canonicalize so the robust compare catches it), no other
Overwrite bypass, false-positive skips are never data loss, Move+Overwrite-onto-self also covered. The
reviewer caught that my first cut over-fired for Keepboth (breaking in-place duplication); fixed by scoping
to Overwrite + added the Keepboth assertion.

## Work Log

- 2026-08-06 — File-ops audit surfaced this CRITICAL data-loss. Fixed with an Overwrite-scoped same_path
  guard in run_transfer; regression-tested (file + folder survival + Keepboth still duplicates); reviewed
  (data-loss airtight; Keepboth over-fire caught + fixed). Shipped in v0.57.58.
