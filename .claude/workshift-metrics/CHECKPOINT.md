# Workshift Checkpoint — 2026-08-01 ~07:40 local

Session: 6815e8b9. Standing mandate (user): build through ALL remaining user-gated epics in a useful
order, Foreman's choice; report a rich plain-language + screenshot summary at EVERY epic boundary;
never wrap-and-ask, roll straight into the next epic.

## State at checkpoint
- `main` @ HEAD `108013ea` (origin/main), CI green, working tree clean.
- **0 open PRs, 0 agent worktrees, backlog quiesced** — clean boundary.

## Merged this stretch (14 PRs) + epic-704 closed
- Epic **CPE-704** (global Spotlight overlay) CLOSED — Reviewer+UAT+Visual PASS.
- #514 CPE-1216 Spotlight overlay · #515 CPE-1219 smoke pin · #516 CPE-1192 archive-browse smoke ·
  #517 CPE-1185 em-dash · #518 CPE-1194 macro-undo fidelity · #519 CPE-1204 near-dup finder ·
  #520 CPE-1220 spotlight highlight polish · #521 CPE-1221 near-dup gui-smoke pin ·
  #522 CPE-1222 tag migration on rename/move · #523 CPE-1184 archive ops via transfer queue ·
  #524 CPE-1193 app-wide dialog borders · #525 CPE-1224 frontend store migration ·
  #526 CPE-1225 snapshot-schedule migration · #528 CPE-1186 extract error precision.

## Open items (Backlog — small QA follow-ups, for the resuming session)
- **CPE-1226** — gui-smoke pin + Visual Critic screenshot for the TransferPanel archive rows
  (build-heavy; also fold in the CPE-1212 danger-badge visual spot-check noted in the ticket).
- **CPE-1227** — add a `do_move_into`/`move_entries` schedule-migration regression test (CPE-1225 gap).

## Deferred
- **CPE-1223** (spotlight basename highlight) — position-filter approach (PR #529, closed) leaves a
  PARTIAL highlight ("marker"->"arker") because the greedy full-path matcher consumes a prefix query
  char; the spotlight gui-smoke catches it. Correct fix documented on the ticket: match the query
  against the BASENAME directly for highlight positions. Low priority.

## NEXT: activate the next epic (PM pick)
Recommended: **CPE-978 (Smart folders & saved searches)** — high user value, buildable headless
(query persistence + a saved-search surface), strong synergy with the search/spotlight/near-dup
infrastructure just built. Alternatives if 978 turns out mostly-built on grep: CPE-688 (explorer
performance 10x), CPE-1000 (true file-type detection & extension-mismatch flagging), CPE-718
(universal thumbnail pipeline). GREP FIRST before decomposing any epic (features are sometimes
already partly shipped).

## Crew defaults that worked this session
- Workers/Reviewers/UAT/combined-checkers: sonnet, `isolation:"worktree"` (ALWAYS isolate anything
  that builds or `gh pr checkout`s — two non-isolated agents on the shared checkout collide, and even
  an isolated agent's first `gh pr checkout` can transiently switch the SHARED branch: verify the
  shared tree afterward). Proportionate gauntlet: full Reviewer+UAT (+Visual for GUI) for substantive
  changes; ONE combined Reviewer+UAT checker for small pure-logic changes mirroring verified patterns.
- Visual Critic for GUI changes = build the PR branch, run the relevant gui-smoke spec to capture a
  real screenshot, dispatch a taste-aware critic to pixel-sample. Stash screenshots to scratchpad
  before pruning the worktree.
- Two recurring worker failure modes to correct via SendMessage immediately: (a) yielding "awaiting
  the background notification" (sub-agents get NONE — tell them to run everything synchronously),
  (b) backgrounding a build + a Monitor and waiting (same fix).
- `gh pr merge --squash --delete-branch` often can't delete the LOCAL branch (used by a worktree) —
  harmless; prune the worktree then `git branch -D`. Windows worktree-dir removal can hit
  "Permission denied" (Defender/handle lock) — `git worktree prune` clears metadata; stale dir is harmless.
- The recurring ` M src-tauri/Cargo.toml` after a checkout is CRLF/LF noise (empty diff) — discard it.
