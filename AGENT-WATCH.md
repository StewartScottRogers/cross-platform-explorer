# Agent Watch (mode)

**Status:** planned / in design.

Agent Watch is a mode of Cross-Platform Explorer, not the app's reason for
existing. The app is a general cross-platform file explorer; Agent Watch is the
view you switch into when an AI coding agent is operating on a directory you
have open.

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
