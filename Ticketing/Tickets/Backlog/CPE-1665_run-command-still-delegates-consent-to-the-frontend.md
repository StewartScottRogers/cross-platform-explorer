---
id: CPE-1665
title: run_command still carries the "the frontend MUST confirm" promise CPE-1651 was filed against — on a command that spawns a shell
type: bug
priority: High
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Found by the independent Security Auditor on PR #844.

`run_command` (`src-tauri/src/lib.rs:7519`, impl at `:7525`) spawns `cmd /C <caller string>`
(`lib.rs:7531-7535`) with a caller-chosen working directory. Its module comment (`lib.rs:7482-7487`) says:

> "This is an external-process launch, so the frontend MUST confirm the resolved command with the user
> BEFORE calling"

That is, verbatim, the pattern **CPE-1651 was filed against** — the backend delegating a safety decision to
the UI — still in place on a command whose primitive is arbitrary code execution rather than deletion:

```js
invoke("run_command", { command: "del /f /s /q \"%USERPROFILE%\\Documents\\*\"" })
```

Any caller that could have forged the old `delete_permanent` can call this instead and get strictly more
than deletion.

## Why it matters

CPE-1651 spent a whole ticket establishing that "the UI must confirm" is not a gate. Leaving the identical
sentence on the most powerful command in the process is the clearest possible instance of the thing that
ticket exists to stop — and it means gating deletion did not raise the attacker's cost.

Be honest about what the fix buys, per the correction already made to `delete_permanent`'s comment: a
`confirmed` boolean is **UI discipline enforced in Rust, not an authorization boundary** — the caller
supplies the flag. It stops a call site that forgets the dialog, and it makes a replayed pre-fix payload
fail deserialization outright. It does not stop a determined attacker already on the IPC surface. Write the
comment that way from the start rather than having it corrected later.

## Scope

1. Add `confirmed: bool` to `run_command`, refusing the whole call up front, matching the shape used by
   `shred_paths` (CPE-1611), `create_vault` (CPE-1630), and `delete_permanent`/`empty_trash` (CPE-1651).
2. Set it **only** in `RunCommandConfirm.svelte` — that dialog already exists as the single confirmation
   point, so this is genuine consent, not the blanket constant the CPE-1651 acceptance criteria forbid.
3. Replace the "the frontend MUST confirm" comment with an accurate statement of what the flag defends.
4. While here: check whether anything else reaches a process launch without passing through that dialog.

## Acceptance criteria

- [ ] `run_command` without the flag is refused before any process is spawned — proved by a test that
      asserts **no child process was created**, not merely that an `Err` came back.
- [ ] The real Run Command flow still works from `RunCommandConfirm.svelte`.
- [ ] The stale "frontend MUST confirm" comment is gone, replaced by an honest description of the gate's
      reach.
- [ ] Neutralise the gate on its own and confirm a distinct test goes red.
- [ ] `bindings.gen.ts` regenerated, or CI's typed-bindings drift guard fails.
- [ ] `delete_permanent`'s doc comment drops this ticket from its ungated-siblings list once this lands.

## Notes

Filed by the Foreman from the PR #844 security audit, 2026-08-12. Same family sweep as **CPE-1662**
(`start_transfer` Overwrite) and **CPE-1664** (`apply_backup_plan`, a verified one-message directory wipe).
Of the three, this one is the smallest change and the largest primitive.
