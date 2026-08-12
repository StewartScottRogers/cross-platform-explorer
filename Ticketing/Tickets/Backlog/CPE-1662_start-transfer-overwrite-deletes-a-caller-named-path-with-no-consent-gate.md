---
id: CPE-1662
title: start_transfer's Overwrite policy deletes a caller-named directory tree with no consent gate
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-12
closed:
---

## Problem

Found by the independent Reviewer on PR #844, while checking whether CPE-1651's sibling audit was
complete. It was not: this is the one file-op command that still destroys an attacker-named path with no
gate at all.

`start_transfer` (`src-tauri/src/lib.rs:2857`) is a **registered IPC command**. It takes `sources`, `dest`
and `policy` straight off the wire. `run_transfer` computes `base_target = dest_dir.join(name)`
(`lib.rs:2792`), and `resolve_conflict` (`lib.rs:2639-2644`) then calls **`fs::remove_dir_all(base_target)`**
when the policy is `ConflictPolicy::Overwrite` and the target is a directory.

So a forged call with `policy: "overwrite"` and a source named `Documents` annihilates
`<dest>/Documents`. That is the same primitive CPE-1651 just closed on `delete_permanent` — except this one
performs **real recursive destruction** rather than a rename, and nothing asks the user.

CPE-1651's acceptance criteria required siblings to be "either fixed here or filed with the finding
recorded". `empty_trash` was found and fixed in that PR; this one was missed, and is filed here.

## Why it matters

This is step 2 of the exploit chain the PR #838 review demonstrated — the ability to make an arbitrary
path disappear on command. CPE-1651 narrowed it; while this remains open, it is not closed. PR #844's doc
comment has already been corrected to say so rather than claiming the primitive is gone.

## Scope

1. **Gate the destructive branch**, mirroring the shape the codebase has now settled on three times —
   `shred_paths` (CPE-1611), `vault_create` (CPE-1630), `delete_permanent` + `empty_trash` (CPE-1651): an
   explicit `confirmed: bool` that refuses the whole call up front, before anything is inspected or
   touched, with a message naming the flag and the one dialog allowed to set it.
2. **Only the Overwrite path needs the gate.** A transfer that does not clobber destroys nothing, so do
   not make every copy/move ask — that would train the user to click through it, which is worse than no
   gate. Gate the branch that actually calls `remove_dir_all`.
3. **Frontend: consent must be a separate argument from intent** — the CPE-1646 lesson. Do not reuse the
   "overwrite" policy value as its own consent; a policy is what the user chose, consent is that they were
   asked.
4. Check the whole conflict-resolution path for the same shape while you are in there — a file overwrite
   that silently replaces a larger file is less catastrophic but the same question.

## Acceptance criteria

- [ ] A `start_transfer` call with `policy: "overwrite"` and no consent flag is **refused up front** —
      nothing copied, nothing deleted, no partial state — and the refusal names what was missing.
- [ ] A test proves it by **reading the victim directory back off disk** after the refused call, including
      a nested file, so the `remove_dir_all` arm is genuinely covered. (Asserting the return value is what
      let this class survive twice.)
- [ ] The consented path still works end-to-end: the real UI overwrite flow completes.
- [ ] Non-overwrite transfers are unchanged and ask nothing new.
- [ ] Neutralise the new gate on its own and confirm a distinct test goes red.
- [ ] `bindings.gen.ts` regenerated (the signature changes), or CI's typed-bindings drift guard fails.
- [ ] `src-tauri/src/lib.rs:2161`'s doc comment on `delete_permanent` is updated to drop the
      "still open (CPE-1662)" paragraph once this lands — and only then.
- [ ] `src/docs/safety-undo.md`'s end-to-end-enforcement section is extended to cover it.

## Notes

Filed by the Foreman from the PR #844 review, 2026-08-12. PR #844 was not blocked on this — it closes a
real hole and its own claims have been corrected to stop overstating the reach. This ticket carries the
part that is genuinely still open.

The reviewer also noted a negligible residual on `move_exact`: a **dangling** symlink at the destination
reads as non-existent, so `fs::rename` would replace it on Unix. Pre-existing and harmless (the link is
already broken), recorded here so it is not re-discovered as new.
