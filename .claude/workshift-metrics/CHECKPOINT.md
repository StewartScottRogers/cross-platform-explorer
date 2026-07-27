# Workshift Checkpoint — resume point for a fresh session

**Written 2026-07-26 ~22:05 local.** The prior session hit the **200-agent sub-agent cap** (the crew can't
spawn mid-session), so it checkpointed here per the budget-reset discipline (`.claude/commands/workshift.md` →
"The sub-agent budget"). **To resume: start a fresh CLI session and say "resume the workshift."** A fresh
session resets the agent budget to 0 and should read this + `history.md` to continue with full context.

Everything below is **merged to `main` and CI-green** unless marked otherwise. Nothing is mid-flight (the tree
is clean, no open PRs, no in-flight gauntlets).

## What shipped last session (context)
- **All 3 user-requested GUI surfaces:** code-preview upgrade, batch-media dialog, Agent-Watch dashboards.
- **4 epics CLOSED:** CPE-724 (code-intel preview), CPE-723 (batch media — added Compress CPE-1103 + optional
  image-overlay Watermark CPE-1106), CPE-728 (activity replay & scrub — event-replay reconstruction).
- **CPE-731 (cost dashboard) = 2 of 3 slices done:** 731a fuller per-session metrics (CPE-1107) + 731b
  per-session metrics journal + flush-on-end (CPE-1113) merged. **731c is the ONLY remaining slice.**
- ~25 gauntlet-merged PRs (#406–#430) + solo work (CPE-1115 skip-UX, 2 QA-Architect integration pins, a docs
  fix). Installed build **0.57.36** is running (has code-preview + batch-media incl. compress/watermark +
  agent-watch dashboards) but **predates CPE-1115** and the QA/doc work — a fresh **0.57.37** release would
  include them.

## NEXT — priority order
1. **CPE-1114 (731c) — cross-session cost dashboard.** The last slice to CLOSE epic CPE-731. Filed + fully
   designed: `Tickets/Backlog/CPE-1114_cost-cross-session-dashboard.md` + Library
   `entries/cost-dashboard-full-ledger-history-plan.md` (§4). Frontend only: a pure `agentMetricsRollup.ts`
   (mirror the tested Rust `fleet_metrics::aggregate` + `efficiency` ratio formulas) + a **History tab** in
   `AgentTimeline.svelte` reading `commands.metricsHistory()` (already merged in 731b) on open (pull-only). Then
   CLOSE CPE-731. **Build this first.**
2. **Optional/low-pri backlog:** CPE-1112 (replay file-pane overlay — optional CPE-728 graduate), CPE-730
   conflict-radar polish (**heat-map** colour-by-owning-agent + **rename-overlap** detection — both decision-free,
   designed in `agent-watch-multisession-actor-plan.md`).
3. **Consider cutting a 0.57.37 release** so the user can see CPE-1115 (loud skip messages) + everything else live.

## Tuned crew defaults (seed the fresh shift with these)
- **sonnet worker + opus reviewer** for GUI/frontend — reviewers caught every rework item; keep opus on review.
- **opus worker** for genuinely-hard slices (hljs-per-line splitter, multi-session watch, TS-fold ports,
  persistence/flush, cfg-gated ledgers) — 0 rework on those.
- **Distinct lib.rs anchor + one-worker-per-file** → zero merge conflicts across the whole run.
- **Bindings serialization:** only ONE bindings-touching backend build in flight at a time (they all regen
  `bindings.gen.ts`); frontend-only + backend can run parallel.
- **Foreman-apply tiny, exactly-prescribed reviewer fixes directly** (re-verify + resume the same reviewer for a
  focused re-check) instead of a full worker round-trip — saves budget.
- **De-risk each hard slice with ONE Plan agent** (read-only) before building — turned the "big" epics (replay,
  cost, conflict-radar) into mostly-wiring. Research/Plan spikes are filed in `.claude/research-library/`.
- **Budget:** reset at ~150/200 agents; this run went to 200 (~25 tickets). Fresh session = full budget.

## Decide-and-log assumptions (carry forward unless the user redirects)
- **Watermark = image-overlay, dependency-free** (not text — text needs a font-rasteriser dep vs the lean-core
  guardrail). Optional: empty overlay → no watermark. (User picked "optional, none if unset"; if they want a
  *text* watermark, that's a new dep decision.)
- **Replay = event-replay**, representation **B** (separate `<session>.baseline.json`, not synthetic events),
  bounded recursive baseline walk. Cost **history = a SIBLING `metrics_journal`** (per-session rows), NOT the
  per-event audit journal (grain mismatch) — same app-data root, same pattern.
- **Cost figures are advisory** (best-effort PTY scrape + derived churn/wall-clock), never billing — keep that
  framing in 731c.

## Open user-facing threads
- CPE-1115 (loud batch-media skip messages) was **Foreman-built solo** (crew capped) — self-verified (check + full
  suite green) but did NOT get the independent Reviewer+UAT gauntlet; eligible for a confirmatory review.
- Leftover `samples/images/pixel-out.png` is an untracked artifact from the user's manual test — harmless; delete
  if a clean tree is wanted.
