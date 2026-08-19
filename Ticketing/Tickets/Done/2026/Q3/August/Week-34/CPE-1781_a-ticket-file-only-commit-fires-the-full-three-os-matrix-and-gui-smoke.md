---
id: CPE-1781
title: A ticket-file-only commit fires the full three-OS matrix and GUI smoke
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-18
closed:
---

## Problem

`ci.yml` and `gui-smoke.yml` both trigger on **every** push to `main` with no `paths-ignore`. So a commit
that touches only `Ticketing/**` — a ticket filed, a status moved to `Done`, a Work Log appended — starts:

- `CI`: frontend type-check + test, and Backend / Server crates / Sidecar platform across **ubuntu, macOS
  and windows** — eleven jobs.
- `GUI smoke`: a shared Linux build plus a four-way shard matrix plus a verdict job.

None of it can possibly be affected by a Markdown file under `Ticketing/`.

Measured during the batched sprint of 2026-08-17/18: the sprint's own bookkeeping pushed **~15 ticket-only
commits to `main` in one night**, each firing both workflows. Because `ci.yml` sets
`cancel-in-progress: true` keyed on the ref, each new bookkeeping push also **cancelled the previous
`main` run mid-flight** — so the runs were not merely wasteful, most never finished.

## Why it is worth fixing rather than tolerating

1. **It competes with the PR runs that matter.** The same account concurrency serves the PRs under review.
   That night saw four GUI-smoke cancellations across four different PRs, at four unrelated steps
   (a poll, dependency install, a shard, and toolchain install in the shared build) — tracked as CPE-1772.
   Whatever the root cause there, `main` burning eleven-job matrices for Markdown edits is capacity taken
   from the queue those PRs are waiting in, and is the one contributing factor that is trivially removable.
2. **It makes `main`'s CI history unreadable.** A long run of `cancelled` conclusions on `main`, all from
   doc commits superseding each other, is indistinguishable at a glance from `main` actually being broken.
3. It is pure waste — three operating systems compiling Rust because a ticket moved folder.

## What to do

- Add a `paths-ignore` to the `push` trigger of `ci.yml` and `gui-smoke.yml` for paths that cannot affect a
  build: `Ticketing/**`, `.claude/**`, and `**/*.md` **except** where a Markdown file is a shipped input.
  **Check that exception carefully before writing it** — `src/docs/*.md` is shipped and guarded by
  `sectionDocs.test.ts`, and `docs/design/*.md` is asserted against by doc-parity tests (CPE-1761 added one).
  A blanket `**/*.md` ignore would silence real guards. When in doubt, ignore only `Ticketing/**` and
  `.claude/**`, which are unambiguous.
- Apply it to `push` only, not `pull_request`. A PR should still get a full run even if its diff is
  docs-only, because the merge result is what ships and a PR is where review happens.
- Verify both directions: a `Ticketing/`-only commit starts nothing, and a commit touching
  `src/docs/*.md` or `docs/design/*.md` still runs the guards that assert on them.
- While there, consider whether `main`'s `cancel-in-progress: true` is right. It is correct for rapid code
  pushes, but it means the last commit of a batch is the only one whose CI result survives. That is a
  reasonable trade — just make sure it is a chosen one, and record why.

## Acceptance criteria

- [ ] A commit touching only `Ticketing/**` triggers neither workflow. Demonstrate with a real commit and
      an empty `gh run list` for it.
- [ ] A commit touching `src/docs/*.md` still runs `sectionDocs.test.ts` and the doc-parity tests.
- [ ] A commit touching `docs/design/*.md` still runs whatever asserts against it.
- [ ] A code commit is entirely unaffected.
- [ ] `pull_request` runs are unchanged.
- [ ] The reasoning for the ignore list, and for keeping or changing `cancel-in-progress`, is recorded in
      the workflow file itself — not only in this ticket.

## Notes

Found by the Foreman during the batched sprint of 2026-08-18, on noticing that every recent `main` run was
either its own bookkeeping or a cancellation of it. Related: CPE-1772 (the GUI-smoke cancellations this
contends with), CPE-1266 (which introduced `cancel-in-progress`), CPE-1753 (the GUI-smoke sharding).

## AC1 demonstrated post-merge, 2026-08-19 — controlled before/after

The acceptance criterion could not be run before the merge: \paths-ignore\ sits on the \pushtrigger, and a push to a PR branch fires \pull_request\ instead, so no push-triggered run existed to
observe. Both the reviewer and the UAT independently reached that conclusion, and the PR body's
original claim that a branch commit could demonstrate it described an impossible test.

Run by the Foreman on \main\ immediately after merge. Two commits of the **same kind** — tickets-only,
no code — thirty minutes apart, either side of the merge:

| Commit | When | Kind | CI | GUI smoke |
|--------|------|------|----|-----------|
| \ea49a9e1\ | 22:08 UTC, **before** the merge | Tickets-only | **fired** | **fired** |
| \832c2672\ | 22:39 UTC, the merge itself | Code (workflows + wdio.conf.ts) | **fired** | **fired** |
| \dfd3c737\ | 22:39 UTC, **after** the merge | Tickets-only | **did not fire** | **did not fire** |

So the filter suppresses exactly what it was meant to and nothing else: a bookkeeping commit starts no
build, while a code commit pushed one minute earlier started the full matrix.

One run does appear against \dfd3c737\: \pages-build-deployment\. That is GitHub's own managed Pages
workflow, not a file in \.github/workflows/\, so \paths-ignore\ does not apply to it and never could.
Recorded here so a future reader who sees a run listed against a ticket-only commit does not conclude
the filter regressed.

**AC1: met.**
