# Agent Watch (mode)

**Status:** built (CPE-396–405, 2026-07-14). Mutations are surfaced live by the filesystem watcher;
file *reads* — which a Windows filesystem watcher can't see — are surfaced too, parsed from the agent's
own tool-output stream and styled distinctly as "consulted" (CPE-405, Done). Remaining read-visibility
polish (a durable per-session consulted-files panel; read-vs-write contrast in the folder heat-map) is
tracked under CPE-726.

Agent Watch is a mode of Cross-Platform Explorer, not the app's reason for
existing. The app is a general cross-platform file explorer; Agent Watch is the
view you switch into when an AI coding agent is operating on a directory you
have open.

## What's built

Triggered by launching a coding agent from the AI Console. All of it is idle-by-default and
feature-gated behind `sidecar-platform`; with no agent running the plain explorer is unchanged.

- **Session registry (CPE-396):** the console announces each session (agent + Project folder) over
  the Status channel; the host forwards it to the explorer.
- **Left-pane "Agents" section (CPE-397):** running sessions listed; click one to navigate into its
  Project folder.
- **Filesystem watcher (CPE-398):** a `notify` watcher on the watched folder streams coalesced
  create/modify/move/delete events (reads excluded — not observable this way). Armed only for a
  session whose project folder the explorer has navigated into at least once this run, and retained
  for that session's whole lifetime once armed (CPE-1606 — see Boundaries below).
- **Live view (CPE-399):** the file list annotates touched rows (kind badge + accent, fading);
  an activity strip names the agent and shows recent changes.
- **Live folder refresh (CPE-401):** created files appear and deleted ones vanish without a manual
  refresh.
- **Folder heat-map (CPE-402):** a folder row lights up when the agent is changing files in its
  subtree, so you can follow it down.
- **Timeline drawer (CPE-400):** a durable, scrollable history of the session's file activity;
  click an entry to jump to its folder.
- **Replay scrubber (CPE-1094):** the drawer's "Replay" tab scrubs a slider back and forth through
  the session's activity timeline, showing what was touched up to the chosen moment and (where
  retained) the diff at that point, with play/pause/step/jump transport. Pure frontend over the
  existing timeline + diff stores — no new backend surface.
- **Cost ledger (CPE-1097/1098):** the drawer's "Cost" tab shows live per-session input/output/total
  tokens and a USD figure, bridged from the sidecar's best-effort PTY usage scrape. These are
  **advisory** — scraped from the agent's own printed output, not a billing record — and cover only
  the 3 fields the scrape captures; the richer `SessionMetrics` ledger (wall-clock, churn, per-model
  rollup, budget) stays unwired dead code (see
  `.claude/research-library/entries/sidecar-live-cost-usage-scanner.md`) until a future ticket wires
  live `RunRecord` capture.
- **Activity-overlap radar (CPE-1099–1101/1100):** the drawer's "Radar" tab folds the (now
  multi-session, actor-tagged) timeline into paths touched by ≥2 distinct actors within a short
  window — the "two agents (or agent vs. user) editing the same file" signal — as a pill list per
  path with a relative timestamp; clicking navigates there. Deliberately worded "activity overlap",
  never "conflict": a raw filesystem watcher can't prove two touches came from unrelated processes vs.
  the same agent revisiting its own file, so an overlap that includes an unresolved `"unknown"` actor
  carries a small hedge note. Pure frontend fold over the existing timeline — no new backend surface,
  no new listener/timer. The honest unknown-vs-agent upgrade (positively confirming a watcher write
  came from an unrelated process) is deferred to a future ticket.
- **Cross-session history dashboard (CPE-1107/1113/1114, epic CPE-731 — closed):** the fuller
  per-session Cost tab (files/edits/churn/wall-clock/throughput, CPE-1107) flushes one
  `SessionMetricsRecord` per session to a persisted `agent-metrics/history.jsonl` on session end
  (CPE-1113, `commands.metricsRecord`/`metrics_history`). The drawer's "History" tab (CPE-1114) reads
  `commands.metricsHistory()` **once, pull-only, on first open this mount** — no listener/timer,
  nothing runs while the tab/drawer is closed — and renders a cross-session rollup (totals, per-model
  and per-agent tables with share, division-safe throughput ratios, and a hand-rolled SVG bars-per-day
  view of cost or tokens). `src/lib/agentMetricsRollup.ts` is the pure rollup, mirroring the tested
  Rust `fleet_metrics::aggregate`/`efficiency` formulas so the numbers match. Same advisory framing as
  the Cost tab — best-effort, never billing.

## What it is for

Agent Watch gives a developer live visibility into the work of an AI coding
agent operating on their codebase. It surfaces every filesystem action the agent
takes — reads, writes, edits, moves, deletes — as it happens, so the user can
follow, understand, and intervene in the agent's work in real time.

## Design tiebreaker (within this mode)

When a choice inside Agent Watch is unclear, pick the option that makes the
agent's activity more visible, sooner. Nothing the agent does should be
invisible.

**This tiebreaker outranks the explorer's** ([PURPOSE.md](PURPOSE.md)) when the
two conflict. Visibility costs memory, CPU, and startup time; inside this mode,
pay the cost. Do not trade away visibility for speed, size, or simplicity.

## Boundaries

- **Off means off.** With Agent Watch disabled, or for a coding-agent project you
  never navigate the explorer into, the plain explorer must still be fast, small,
  and predictable: no watcher, no background polling, no startup penalty. This is
  the one constraint the mode may not spend.
  - CPE-1099 added concurrent multi-session watching (the Radar/Cost/History tabs
    fold data across every session, not just the on-screen one) — but a naive
    "watch every currently-running session unconditionally" reading of that
    briefly violated the boundary above: it armed a filesystem watcher for a
    project the explorer had *never* opened, for as long as that agent session
    ran (CPE-1606). The fix keeps the multi-session value without spending the
    boundary: a session's watcher only arms once the explorer has navigated into
    its project folder at least once **this run** (`markVisited` in
    `src/lib/agentSessions.ts`). A session never opened stays fully idle — genuinely
    off means off.
  - Once a project has been visited, its watcher is **retained** for the rest of
    that session's life, even after the explorer navigates elsewhere — including
    to a sibling agent's project. This is deliberate, not a residual leak: tearing
    the watch down on every navigation would thrash a `notify` watcher on rapid
    back-and-forth between sibling projects, and would prematurely flush that
    session's Cost/History row as if it had ended (`reconcileAgentWatch` flushes
    on removal), fragmenting one live session into two metrics rows. Ending the
    agent session (not just navigating away) is what actually stops watching a
    visited project — see `src/docs/explorer-agent-watch.md` for the user-facing
    framing and the `markVisited` doc comment for the full reasoning.
- It observes; it does not drive the agent. No agent control surface lives here.
- It should be implementable as an additive layer over the existing filesystem
  commands rather than a rewrite of them.
