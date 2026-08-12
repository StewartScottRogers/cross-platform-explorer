---
id: CPE-1654
title: A refused vault lock is reported as "files in use" and navigates into the tampered path
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-11
closed:
---

## Problem

Two loose ends from the independent re-review of PR #838 (CPE-1647). Neither was in that ticket's scope;
both were reported rather than scope-crept, and neither puts data at risk.

### A. The frontend mislabels a containment refusal, then navigates into the tampered path

`src/App.svelte:2456-2464` surfaces **every** failed lock as *"some files may still be in use. Try
again."* and then navigates back into `sessionDir`. After CPE-1647 there is a second, quite different
failure: the backend refused because the session path no longer resolves inside the vault-sessions root
(someone swapped it for a link). In that state the message is wrong — trying again will never help — and
the navigation drops the user into whatever the link points at (their own Documents, in the demonstrated
exploit) while the UI still shows a "vault unlocked" banner, even though the backend has already dropped
the mapping.

It self-heals on a second Lock click (the backend no-ops and returns `Ok`, clearing the store), and it
only arises in an attacker-constructed state — hence Low. But the app should tell the truth: a tamper
refusal is not a busy-file, and the UI should not follow the tampered path.

### B. `docs/design/VAULT-SECURITY.md` §7 states the wrong symptom for one of the two guards

§7 says that with **either** guard removed the tests fail with *"it was DESTROYED"*. That is true for the
`wipe_session_dir` symlink refusal. Remove the lock-time containment re-check instead and the two swap
tests fail on the *wedged-unlocked* assertion (`vault_manager.rs:1344`) — because the other guard still
saves the bytes. The substantive claim (each guard is independently pinned red by the suite) is correct
and was verified; only the stated symptom is wrong for one of them. §5's guarantee statement is accurate
and should be left alone.

## Acceptance criteria

- [ ] A containment/tamper refusal from `vault_lock` is distinguishable in the frontend from a transient
      wipe failure, and gets its own honest message — no "try again" for a state where retrying cannot help.
- [ ] On a tamper refusal the app does NOT navigate into `sessionDir`, and the "unlocked" banner is cleared
      to match the backend, which has already dropped the mapping.
- [ ] A transient failure (file genuinely in use / read-only) keeps its current retry-friendly message and
      behaviour — the re-run UAT confirmed that path works and it must not regress.
- [ ] `docs/design/VAULT-SECURITY.md` §7 states the correct symptom per guard.
- [ ] `npm run check` + vitest green; a test covers the two failure shapes landing on different messages.

## Notes

- Source: independent re-review findings 1 and 3 on PR #838 (CPE-1647), 2026-08-11.
- Related: [[CPE-1647]] vault session containment, [[CPE-1653]] refused lock leaves link debris,
  [[CPE-1645]] locking a vault destroys edits made while unlocked.
- CPE-1645 rewrites what locking does, so sequence this after it or design them together — otherwise the
  message work may be redone.

## Work Log

- 2026-08-11 — Filed by the Foreman from the PR #838 re-review.
- 2026-08-11 — Fixed in the [[CPE-1645]] PR, sequenced **after** that ticket's product decision as the
  notes asked, so the message work was done once against the final set of failure shapes (there are now
  three, not two — re-sealing can fail too).

  **A.** New pure `classifyLockError(raw, name)` in `src/lib/vaultStore.ts` returns `{ kind, retryable,
  message }` for `tamper` / `reseal` / `transient`. It classifies on wording the backend produces
  deliberately: `UNTRUSTED_SESSION` (a new const in `vault_manager.rs` carried by every containment/link
  refusal and **only** those — its doc comment names this function, and vice versa) and `reseal_failed`.
  Ordering is load-bearing and pinned by a test: the tamper copy itself contains the words "nothing was
  re-sealed", so the tamper check must run first or it would be mis-sorted as retryable.
  - `tamper` → `retryable: false`, its own honest copy (no "try again", no "files in use"), it says
    nothing was deleted and that the vault is sealed and now shown as locked. `lockVault` clears the store
    entry for this kind only — matching the backend, which has already dropped its mapping — so the
    banner goes away; and `App.lockActiveVault` no longer navigates back into `sessionDir`, which is the
    bug: that path resolves somewhere else entirely (the user's own Documents, in the demonstrated
    exploit).
  - `transient` keeps the exact previous copy and behaviour (navigate back in, retry) — the re-run UAT
    confirmed that path works, and a test pins it.
  - `reseal` (new, from CPE-1645) is retryable, navigates back in, and says the changes couldn't be saved
    back but that nothing was deleted and the files are still in the unlocked folder.

  **B.** Corrected — with one clarification: the wrong claim is in **§6** (the adversarial review record's
  CPE-1647 review #1 bullet), not §7; the ticket's section reference was off by one. It now states the
  symptom per guard: removing `wipe_session_dir`'s symlink refusal fails the swap tests on
  `assert_precious_intact` ("it was DESTROYED"), while removing the lock-time containment re-check fails
  them one assertion later on the wedged-unlocked check, because the other guard still saves the bytes.
  §5's guarantee statement was left alone as instructed.

  **Red→green.** `a tamper refusal clears the vault entry…` fails on the unfixed store (`expected true to
  be false` — the vault stays "unlocked" with a live banner), and the classifier test covers all three
  shapes landing on different messages and different recovery. A control test asserts a re-seal failure
  keeps the vault unlocked. Deleting the tamper branch from `classifyLockError`, or the store-clearing in
  `lockVault`, turns them red again (verified separately).
