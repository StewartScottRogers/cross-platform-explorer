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
  create/modify/move/delete events (reads excluded — not observable this way).
- **Live view (CPE-399):** the file list annotates touched rows (kind badge + accent, fading);
  an activity strip names the agent and shows recent changes.
- **Live folder refresh (CPE-401):** created files appear and deleted ones vanish without a manual
  refresh.
- **Folder heat-map (CPE-402):** a folder row lights up when the agent is changing files in its
  subtree, so you can follow it down.
- **Timeline drawer (CPE-400):** a durable, scrollable history of the session's file activity;
  click an entry to jump to its folder.

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

- **Off means off.** With Agent Watch disabled, the plain explorer must still be
  fast, small, and predictable. Watchers idle, no background polling, no startup
  penalty. This is the one constraint the mode may not spend.
- It observes; it does not drive the agent. No agent control surface lives here.
- It should be implementable as an additive layer over the existing filesystem
  commands rather than a rewrite of them.
