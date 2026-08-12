---
id: CPE-1651
title: delete_permanent trusts the UI to have confirmed — no backend gate
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-11
closed:
---

## Problem

`delete_permanent` (`src-tauri/src/lib.rs:2111`, registered around `lib.rs:11335`) permanently deletes
every path it is handed, with **no backend confirmation gate**. Its own doc comment says "the UI must
confirm" — i.e. the backend delegates the safety decision to the frontend.

That is exactly the class of assumption CPE-1647 was filed against, and the CPE-1611 (`shred_paths`)
and CPE-1630 (`vault_create`) tickets already established the house rule: an irreversible command must
carry its own explicit-consent parameter, because the IPC boundary is reachable without the UI.

It is not theoretical. The independent review of PR #838 used `delete_permanent` as **step 2 of a
working exploit chain**: unlock a vault into its legitimate session dir → `delete_permanent` that dir →
plant a junction at the same path pointing at the user's Documents → lock the vault → the shredder
walks the junction and destroys the victim's files. The reviewer demonstrated the shred with a probe
test, not by argument.

CPE-1647 closes the vault half of that chain. This ticket closes the primitive that made step 2 free.

## Acceptance criteria

- [ ] `delete_permanent` takes an explicit consent parameter and refuses without it, matching the shape
      already used by `shred_paths` (CPE-1611) and `vault_create` (CPE-1630) — same refusal style, same
      error type, so the three read consistently.
- [ ] The refusal is a clean, distinguishable error, not a panic, and deletes nothing on the refused path.
- [ ] The frontend call sites pass consent only where the user genuinely confirmed; no blanket
      always-true constant threaded through to satisfy the compiler.
- [ ] A test proves the un-consented call deletes nothing, verified by reading the files back off disk
      (not by asserting the error alone).
- [ ] Audit the sibling commands in the same family for the same assumption — at minimum `move_exact`,
      which the reviewer noted "works equally well" as the exploit's step 2 — and either fix them here
      or file them with the finding recorded.
- [ ] `cargo clippy --all-targets -D warnings` in both feature modes, plus the crates/server test suite.
- [ ] If a `specta::Type` struct changes, `src/lib/bindings.gen.ts` is regenerated (CI drift guard).

## Notes

- Source: independent reviewer Finding 1 on PR #838 (CPE-1647), 2026-08-11 — the exploit chain, not a
  code-reading hunch.
- Related: [[CPE-1611]] shred_paths confirm gate, [[CPE-1630]] vault_create confirm gate,
  [[CPE-1647]] vault session containment, [[CPE-1642]] output identity resolution.
- Sequencing: independent of CPE-1647; both should land. CPE-1647 alone is not sufficient, because the
  junction-swap primitive is generic — any command that removes a directory the app later writes to
  can play step 2.

## Work Log

- 2026-08-11 — Filed by the Foreman from the PR #838 review finding.
