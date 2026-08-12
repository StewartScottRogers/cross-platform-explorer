---
id: CPE-1664
title: apply_backup_plan recursively wipes any directory the caller names, from a single IPC message
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-12
closed:
---

## Problem

Found by the independent Security Auditor on PR #844, and **verified with a passing exploit test**
(`probe_apply_backup_plan_dot_wipes_the_entire_dest_root`).

`apply_backup_plan` (`src-tauri/src/lib.rs:3403`, and its streaming twin at `:3372`) is registered in
`generate_handler!` at `lib.rs:11364`. It reaches `std::fs::remove_dir_all` / `remove_file`
(`crates/server/src/backup.rs:82-86`) on a root the caller chooses freely, with **no consent parameter to
withhold** and no app-op ledger note. It is strictly more powerful than `delete_permanent` was before
CPE-1651 gated it.

```js
invoke("apply_backup_plan", {
  sourceRoot: "",
  destRoot:   "C:\\Users\\me\\Documents",
  copy: [], update: [],
  deletePaths: ["."],          // one entry
  verify: false
})
```

`safe_join` permits `Component::CurDir` (`backup.rs:22`), so `"."` resolves to `dest_root` itself,
`dst.is_dir()` is true, and `remove_dir_all(dest_root)` recursively annihilates the tree. One IPC message,
arbitrary directory, no Recycle Bin copy, no confirmation anywhere in the stack. The narrower form
(`deletePaths: ["taxes.docx"]`) is verified too.

Traversal itself **is** blocked — `..` and absolute paths are correctly rejected
(`probe_safe_join_blocks_dotdot_but_dest_root_is_unvalidated` passes). The escape hatch is `dest_root`,
which is never validated at all.

## Why it matters

This is the same class CPE-1651 just closed on `delete_permanent`, and worse: recursive, unlogged, and
requiring no dialog to have ever existed. Gating one destructive command while this stays open does not
raise an attacker's cost, which is exactly why CPE-1651's doc comment has been corrected to name it.

## Scope

1. **Add `confirmed: bool`** to both `apply_backup_plan` and its streaming twin, with the refusal shape the
   codebase has now settled on four times (`shred_paths` CPE-1611, `create_vault` CPE-1630,
   `delete_permanent` + `empty_trash` CPE-1651): refuse the whole call up front, before anything is
   inspected, naming the flag and the one dialog allowed to set it. Thread it from `BackupDashboard.svelte`'s
   run confirm (`src/lib/components/BackupDashboard.svelte:95`, `src/App.svelte:605`) — a real user gesture,
   not a hard-coded constant.
2. **Independently, reject `Component::CurDir` in `safe_join`.** A backup-plan entry naming the root itself
   is always malformed, gate or no gate. Do both — the gate is UI discipline, this is a correctness fix.
3. Consider whether `dest_root` deserves any validation of its own (it currently has none).

## Acceptance criteria

- [ ] The exploit call above is refused with nothing deleted; a test proves it by **listing the directory
      back off disk** afterwards, including a nested file.
- [ ] `deletePaths: ["."]` is rejected by `safe_join` itself, independently of the consent flag — verified
      by a test that calls `safe_join` directly.
- [ ] The real backup flow still works end to end from the dashboard.
- [ ] Neutralise each new guard on its own and confirm a distinct test goes red for each.
- [ ] `bindings.gen.ts` regenerated (signatures change), or CI's typed-bindings drift guard fails.
- [ ] `delete_permanent`'s doc comment (`lib.rs`) drops this ticket from its ungated-siblings list once
      this lands — and only then.

## Notes

Filed by the Foreman from the PR #844 security audit, 2026-08-12. The auditor's probes are at
`…/scratchpad/sec_audit_844_probe.rs` — drop into `crates/server/tests/` to re-run; 6/6 reproduce.

Related, same family sweep: **CPE-1662** (`start_transfer`'s Overwrite policy) and **CPE-1665**
(`run_command`). The auditor also flagged `checkpoint_revert`
(`lib.rs:4262` → `crates/server/src/revert_engine.rs:157`), which mass-`remove_file`s every file under a
caller-chosen root that is absent from the named manifest — bounded by requiring a pre-existing checkpoint
for that exact root, and semantically a restore, so it was not rated. It belongs in this sweep.
