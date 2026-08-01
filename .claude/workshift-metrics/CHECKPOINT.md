# Workshift Checkpoint — 2026-08-01 ~12:45 local

Session: 6815e8b9. Standing mandate (user): build through ALL remaining user-gated epics in a useful
order, Foreman's choice; rich plain-language + screenshot report at EVERY epic boundary; never
wrap-and-ask — roll into the next epic (reset via fresh-session resume before the ~200-agent/session
wall per [[workshift-subagent-budget-reset]]).

## State — CLEAN BOUNDARY
- `main` @ HEAD `26b715bc` (origin/main), working tree clean. **0 open PRs, 0 agent worktrees.**
- NOTE: GitHub runners badly backlogged (jobs queued 1h+). #537's cross-OS CI hadn't finished when
  merged; merged on full local verification + pure-Rust dep analysis. Re-check main CI when it clears;
  fix-forward if red (unlikely — pure-Rust deterministic).

## THREE epics delivered this session + backlog cleared (~24 PRs merged, #514–#538)
- **CPE-704** (global Spotlight overlay) — DONE + reported.
- **CPE-978** (Smart folders & saved searches) — DONE + reported.
- **CPE-718** (Universal thumbnail pipeline) — HEADLESS SLICE done + reported (CPE-1236 SVG/font
  extractors #537, CPE-1237 streaming client #538, Visual PASS). Reverted to Proposed; remainder
  (video/PDF/office = CPE-1238) is heavy-dep + USER-GATED (dependency-weight decision).

## Open Backlog (small follow-ups)
- CPE-1226 — TransferPanel archive-row gui-smoke pin + Visual Critic (build-heavy) + the CPE-1212
  danger-badge visual spot-check.
- CPE-1235 — parentDir POSIX-root edge (live-refresh). Low.
- CPE-1239 — thumbnail retry-cap regression-pin test + spawn_blocking-panic cancel-leak. Low.

## Deferred
- CPE-1223 — spotlight basename-scoped highlight (needs basename-DIRECT matching). Low.
- CPE-1238 — video/PDF/office thumbnails (heavy deps; USER dep-weight decision). Big-design.

## NEXT EPIC (scout on resume — pick the best cleanly-headless one)
Run a read-only PM scout (Explore agent) over the still-unbuilt candidates, GREP-FIRST (features here
are often partly-built): **CPE-713 (tray resident), CPE-714 (terminal dock), CPE-716 (drive bay),
CPE-738 (secure delete & encrypted vaults), CPE-661 (universal drag-and-drop), CPE-712 (shell citizen
/ OS context-menu), CPE-729 (intervene & approve — gate agent actions), CPE-616 (remote/cloud fs)**.
Pick the one with the best (high value × cleanly crew-buildable × real unbuilt gap). Prefer ones NOT
needing a model key / real hardware / cert. Known user-gated (skip unless user provides): CPE-976/977/
979/980 (AI — model key), CPE-717 (SFTP/Mac), CPE-1238 (heavy deps), Mac/cert items. Known
effectively-done: CPE-688 (perf — only a real-hardware 10× benchmark left), CPE-1000 (file-type — only
a tiny mismatch-review filler left).

## Crew defaults / lessons (unchanged from prior checkpoint — still all apply)
- Workers/Reviewers/UAT/checkers: sonnet, `isolation:"worktree"` for anything that builds or
  `gh pr checkout`s. An agent's first `gh pr checkout` often transiently switches the SHARED checkout's
  branch (`gh` evades the git cd-guard) — ALWAYS `git checkout main` on the shared tree before merging;
  discard the recurring empty ` M src-tauri/Cargo.toml` CRLF noise.
- Proportionate gauntlet: full Reviewer+UAT (+Visual for GUI) for substantive; ONE combined checker for
  small pure-logic mirroring verified patterns; Foreman-verify test-only PRs by reading the diff.
- Visual leg = build PR branch → run the relevant gui-smoke spec → capture PNG → taste-aware Critic
  pixel-samples; stash the PNG to scratchpad before pruning. New surface w/ no spec → file a gui-smoke-pin
  follow-up (CPE-1221/1233 pattern).
- Worker failure modes to correct via SendMessage immediately: (a) "awaiting the background
  notification", (b) backgrounding a build + a Monitor and waiting — sub-agents get NO background
  notifications; run everything SYNCHRONOUSLY in the foreground.
- `gh pr merge --squash --delete-branch` often can't delete the LOCAL branch (worktree holds it) —
  prune worktree then `git branch -D`. Windows worktree-dir removal can hit "Permission denied"
  (Defender/handle) — `git worktree prune` clears metadata; stale dir harmless.
- Children overlapping App.svelte/Sidebar.svelte MUST be built sequentially.
- New Rust deps: watch the 3-OS CI; but pure-Rust deterministic crates build identically cross-OS, so a
  thorough Reviewer dep-analysis can substitute if the runner queue is stalling the shift.
