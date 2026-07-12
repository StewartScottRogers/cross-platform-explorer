# Purpose

Cross-Platform Explorer is a fast, native file explorer that behaves the same on
Windows, macOS, and Linux. Its purpose is to let a user see and move through a
filesystem with as little friction as possible — small binary, quick start,
no surprises, platform conventions respected.

**Tiebreaker:** When a design or implementation choice is unclear, favour the
option that keeps the app fast, small, and predictable over the one that adds
capability.

## Modes

The explorer is the product; modes are additive views layered over it. A mode
must never degrade the core explorer experience when it is switched off.

- **[Agent Watch](AGENT-WATCH.md)** — a live view of an AI coding agent's
  filesystem activity, for following the agent's work in real time. Has its own
  design tiebreaker, scoped to that mode only.

## Precedence

The two tiebreakers above will collide — visibility costs memory, CPU, and
startup time. When they do:

**Agent Watch wins.** Seeing what the AI is doing is the more important of the
two. If making the agent's activity visible costs speed, size, or simplicity,
pay the cost.

This precedence is bounded by one rule, and only one: with Agent Watch switched
**off**, the explorer must still be fast, small, and predictable. Inside the
mode, visibility is the priority; outside it, the explorer's tiebreaker holds.
