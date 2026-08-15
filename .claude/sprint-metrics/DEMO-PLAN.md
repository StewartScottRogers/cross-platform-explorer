# Demo plan — the dog-and-pony show

Requested 2026-08-15 by the user, who is low on tokens and expects to run out **Monday**. So this is
scheduled for **today/tomorrow**, not Monday, and the sprint bound was cut **40 → 36 batches** to reserve
budget for it. If a session reset happens before the show, this file is the handoff.

## Trigger

When the four in-flight PRs land (CPE-1753 sharding, CPE-1757 bidi guard, CPE-1756 copy-inflow,
CPE-1744 extraction sinks) — batch ~36 — **stop dispatching new tickets** and run the show.

Do not wait for a tidier milestone. The user's budget is the binding constraint, not the queue.

## The honesty problem, and how the show handles it

Almost everything this sprint fixed is **invisible in a screenshot**. Twelve of thirteen merged tickets are
cases of the app doing something wrong *and reporting success*: a truncated download written to disk as a
finished file, an archive writing outside the folder you chose, the Copilot trashing the folder you
confirmed, a save stripping a file's permissions and its downloaded-from-the-internet mark.

A gallery of screenshots would misrepresent that work. The show is therefore **before/after
demonstrations**, each one a thing that used to happen and now doesn't, with the measured evidence beside
it. One fix (the bidi filename spoof) *is* genuinely visual and leads, because it is the one a person can
see and immediately understand.

## Shape

1. **Live app, running.** Full build → install the sidecar build → launch, per
   [[gui-verify-needs-build-deploy-run]] and [[always-install-sidecar-build]]. Kill every `cpe` /
   `ai-console` process first ([[install-kill-all-processes-first]]) or NSIS silently skips the locked
   sidecar. Bracket with the WAIT → BUILD → DEPLOY → RUN → RUNNING narration.
2. **The one you can see.** A folder containing a file whose name carries a right-to-left override.
   Before: it draws as `photo.png`. After: `[RLO]gnp.txt`. Beside it, a real Arabic and a real Hebrew
   filename rendering untouched — the half that makes the fix correct rather than merely safe.
3. **The five you can't see, as before/after pairs**, each with the actual measured numbers already in
   the tickets: truncated download, archive write-through, Copilot folder deletion, save stripping
   attributes, S3 listing rendered short. Each states what the app used to answer (`Ok`, success) versus
   what it answers now.
4. **What it cost and what it caught.** From `ledger.jsonl`: agents, rounds, and the count that matters —
   **every one of the four worst defects was introduced by a fix and caught by an independent check before
   merge.** That is the argument for the two-check gate, and it is the most persuasive thing in the run.
5. **What is still open**, named honestly: CPE-1753's cap arithmetic if sharding didn't land, the
   extraction gaps CPE-1744 leaves, and the 14 tickets filed from review findings.

## Deliverable

A published **Artifact** (private page, user shares if they want) — load the `artifact-design` skill first.
Plus the running app on screen. The artifact is the thing that survives the user running out of tokens;
build it to be readable cold, a week later, by someone who was not here.

## Cost discipline from now until the show

- No new tickets dispatched after the current four.
- Reviews stay on the two-check gate — that is what caught the four introduced defects and is not where to
  economise.
- Prefer resuming an existing reviewer/UAT over spawning a fresh one; they hold their probes.
