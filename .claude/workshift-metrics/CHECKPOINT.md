# Workshift Checkpoint — 2026-08-01 ~10:10 local

Session: 6815e8b9. Standing mandate (user): build through ALL remaining user-gated epics in a useful
order, Foreman's choice; rich plain-language + screenshot report at EVERY epic boundary; never
wrap-and-ask — roll into the next epic (budget permitting; reset via fresh-session resume before the
~200-agent/session wall per [[workshift-subagent-budget-reset]]).

## State at checkpoint — CLEAN BOUNDARY
- `main` @ HEAD `5b47add2` (origin/main), CI green, working tree clean.
- **0 open PRs, 0 agent worktrees, nothing in flight.**

## Two epics closed this session
- **CPE-704** (global Spotlight overlay) — DONE + reported.
- **CPE-978** (Smart folders & saved searches) — DONE + reported (this boundary). Children merged:
  1228 store · 1229 save+sidebar+open-evaluator · 1230 live-refresh · 1231 reorder · 1233 gui-smoke pin
  · 1234 preview-icon fix. Visual PASS. (1230 bounced once — 2 defects — fixed + re-verified.)
- Plus the entire working backlog cleared (~20 PRs merged total: #514–#536).

## Open Backlog (small follow-ups, for the resuming session)
- **CPE-1226** — gui-smoke pin + Visual Critic for the TransferPanel archive rows (build-heavy; also
  fold in the CPE-1212 danger-badge visual spot-check noted in that ticket).
- **CPE-1235** — `parentDir` returns "" for a POSIX file at the volume root (live-refresh edge; Windows
  unaffected). Low.

## Deferred
- **CPE-1223** — spotlight basename-scoped highlight: position-filter leaves a partial highlight; needs
  basename-DIRECT matching (documented on the ticket). Low.

## NEXT EPIC (PM pick — activate on resume)
Recommended: **CPE-688 (Explorer performance — 10× faster directory open & all file-list ops)** — high
user value, aligns squarely with PURPOSE's fast/small/predictable tiebreaker, buildable headless
(profiling + backend/frontend perf work). Strong alternative: **CPE-1000 (true file-type detection &
extension-mismatch flagging)** — buildable backend, high utility. Also open: 718 (thumbnails), 720
(a/v player), 725 (media metadata studio — MetadataStudioDialog partly exists, GREP FIRST), 713 (tray),
714 (terminal dock), 716 (drive bay), 729 (agent gate), 616 (remote fs), 738 (secure delete/vaults),
976/977/979/980 (AI — need a model key/provider seam), 712 (shell citizen), 661 (drag-drop).
**GREP FIRST before decomposing any epic** — features are often already partly shipped (e.g. CPE-978's
model was orphaned-but-built; 725 has a MetadataStudioDialog). Do a read-only DoD-gap assessment
(Explore agent) → decompose only the real gap → build in bounded batches.

## Crew defaults / lessons that worked this session
- Workers/Reviewers/UAT/checkers: sonnet, `isolation:"worktree"` for anything that builds or
  `gh pr checkout`s. NOTE: an agent's first `gh pr checkout` often transiently switches the SHARED
  checkout's branch (the git cd-guard doesn't catch `gh`) — ALWAYS `git checkout main` on the shared
  tree before merging, and discard the recurring empty ` M src-tauri/Cargo.toml` CRLF noise.
- Proportionate gauntlet: full Reviewer+UAT (+Visual for GUI) for substantive changes; ONE combined
  Reviewer+UAT checker for small pure-logic changes mirroring already-verified patterns; Foreman-verify
  test-only PRs by reading the diff.
- Visual leg for GUI = build the PR branch, run the relevant gui-smoke spec to capture a real
  screenshot, dispatch a taste-aware Critic to pixel-sample; stash the PNG to scratchpad before pruning.
  For a brand-new surface with no spec, file a gui-smoke-pin follow-up ticket (CPE-1221/1233 pattern).
- Two worker failure modes to correct via SendMessage immediately: (a) "awaiting the background
  notification" and (b) backgrounding a build + a Monitor and waiting — sub-agents get NO background
  notifications; tell them to run everything SYNCHRONOUSLY in the foreground.
- `gh pr merge --squash --delete-branch` often can't delete the LOCAL branch (worktree holds it) —
  harmless; prune worktree then `git branch -D`. Windows worktree-dir removal can hit "Permission
  denied" (Defender/handle) — `git worktree prune` clears metadata; stale dir is harmless.
- Sequential vs parallel: children that overlap App.svelte/Sidebar.svelte MUST be sequential.
