---
title: Agent Watch
order: 51
category: Explorer
categoryOrder: 2
---

# Agent Watch

Agent Watch is a mode layered over the explorer — not a separate app, and not a toggle you hunt for in
Settings. It gives you live visibility into what an AI coding agent is doing to your files as it works,
so you can follow, understand, and (via checkpoints) recover from its changes. The repository's
`AGENT-WATCH.md` describes the design intent behind it; this page documents what actually ships,
including a few places the two have drifted apart — called out under *Limits / notes* below.

## When it appears

Launch a coding agent from the AI Console (Agent Deck), then navigate the explorer into — or already be
sitting in — that agent's project folder. An **"Agent Watch — ⟨name⟩"** strip appears above the file list
with a pulsing live dot, a running row of recent change chips (`+ name` created, `~ name` modified,
`− name` removed; each fades after ~6 seconds), and a **Log** button. If you're nested several folders
deep inside more than one running agent's project (one watching a parent, another its subfolder), the
**deepest** matching project wins.

Files the agent has touched show a small badge in the file list itself while you're inside the watched
folder, and a folder row lights up (a "heat map") when the agent is changing something in its subtree —
so you can follow it down without opening every level. When more than one actor (agent or you) has
touched a subtree recently, a colour-coded legend below the list maps each colour to its actor.

## The sidebar Agents section

Every running session — whether or not you're currently looking at its folder — appears under
**Agents** in the sidebar, each with a small numbered colour chip (the same colour used to identify that
session throughout Agent Watch, e.g. in the Radar tab). Click a row to navigate there; double-click to
open its folder in a new tab; right-click for more options.

## The Log drawer

Click **Log** to open a drawer on the right with five tabs: **Live**, **Replay**, **Cost**, **Radar**,
**History**.

### Live

The default tab: a durable, newest-first log of every filesystem action the agent has taken this
session — created, edited, removed, moved (renamed). Click a row to jump the explorer to its containing
folder. A row whose edit captured a before/after diff shows a `+added −removed` line-count summary;
hover or focus it to peek the diff inline, or click **Open full diff** for a side-by-side view.

Above the log, a collapsible **Consulted** panel lists files the agent has *read* (not just changed) this
session — reads aren't visible to a filesystem watcher, so they're parsed from the agent's own tool-output
stream instead and shown with a dim, hollow "read" badge, distinct from the change badges. A file read
more than once shows a `×N` count. Click an entry to jump there, same as a Live row.

### Replay

Scrub back and forth through the session's recorded activity with a slider, plus jump-to-start,
jump-to-end, step back/forward, and play/pause at **0.5×/1×/2×/4×** speed. Moving the scrubber
reconstructs the folder listing exactly as it stood at that instant — files created, modified, or removed
by that point are shown read-only right in the drawer, each flagged with its last-touch kind (or
**existing** for something that predates the session). A **"Show in file pane"** checkbox graduates that
same reconstruction into the *main* explorer pane — pausing its live listing until you switch the toggle
off or leave the Replay tab. If a path was edited more than once during the session, its diff at an
earlier scrub point isn't retained; the drawer says so ("content at this point not retained — showing
latest") rather than showing a diff from the wrong moment.

**Checkpoint markers.** Any [checkpoint](16-checkpoints) taken for the folder you're watching shows up as
a pin on the scrubber track. Click one to open a compact restore panel: a preview of what reverting would
do (files restored / overwritten / deleted, total bytes, and a **drift count** — files changed *since*
that checkpoint that reverting would clobber), a highlighted list of the drifted paths when there are any,
and a two-step confirm ("Revert to this checkpoint…" then "Yes, revert") before anything is written. This
is the same underlying mechanism [Checkpoints & Rollback](16-checkpoints) uses elsewhere in the app,
surfaced here so a bad agent edit is one click away from undone.

### Cost

Live, per-session token and cost usage for every reporting session (not just the one you're watching):
input/output/total tokens and a USD estimate, plus files touched, edit count, churn (bytes changed), and
wall-clock time, with per-minute/per-file throughput ratios once there's enough data to compute them. A
banner is explicit that these are **best-effort figures scraped from the agent's own printed output — not
a billing record** — and that files/edits/churn/wall-clock are approximations derived from what this app
itself observed, not an authoritative agent-side count.

### Radar

Folds the (multi-session) activity log into paths touched by **two or more distinct actors** within a
short window — the "two agents, or an agent and you, editing the same file" signal — as a list with the
actors involved and a relative timestamp; click a row to jump there. It's deliberately worded as an
**activity overlap**, not a "conflict": a filesystem watcher can't prove two touches came from genuinely
unrelated processes rather than the same agent revisiting its own file, so an overlap involving an
unresolved actor carries a hedge note. A separate **Competing renames** section flags same-source or
same-destination rename divergences across distinct actors the same way.

### History

A cross-session rollup read from a small local history log, loaded once the first time you open the tab
(not on a timer): totals (sessions, cost, tokens, time, files touched, churn), throughput ratios,
per-model and per-agent breakdowns with each one's share of total cost, and a bar chart of cost or tokens
per day. Same advisory framing as the Cost tab.

## Session history (browse + export)

A separate, palette-only tool — **Session history…** (Command Palette) — browses the durable on-disk
audit journal directly, independent of whether anything is currently being watched or whether the drawer
is even open. Pick a past session from the list, filter its events by kind (created/modified/removed/
renamed/read) and a path-contains substring, optionally redact paths under your home directory, and
export the filtered set as **JSON**, **CSV**, or **Markdown**.

## Worked example

You've asked a coding agent to refactor a module and want to watch it work without losing your own place
in the explorer.

1. Launch the agent from the AI Console against your project folder; navigate the explorer there (or
   you're already there) — the Agent Watch strip appears.
2. Watch change chips accumulate in the strip as it edits files; click **Log** to open the drawer.
3. Hover a Live-tab row with a diff summary to peek what changed inline, or click **Open full diff** for
   the side-by-side view.
4. If an edit goes wrong, switch to **Replay**, find the checkpoint pin from just before the bad edit, and
   use **Revert to this checkpoint…** — the drift preview shows exactly what would be undone before you
   confirm.

## Limits / notes

- **"Off means off" holds for a project you never open — a session you never navigate the explorer
  into stays completely unwatched, no matter how long it runs (CPE-1606).** The nuance is what happens
  once you *do* open it: the underlying `notify` watcher stays armed for that project for the rest of
  the session's life, even after you navigate elsewhere — including to a sibling agent's project — so
  the Radar/Cost/History tabs keep working across every project you've actually looked at this run, and
  a quick hop between two sibling agent folders doesn't repeatedly tear down and re-arm a watcher.
  Leaving the folder only hides the strip and file-list badges; ending the agent session (not just
  navigating away from a project you've visited) is what actually stops watching it. See
  `AGENT-WATCH.md`'s Boundaries section for the full reasoning.
- **Advisory numbers, never billing.** Every dollar/token figure across Cost and History is scraped from
  the agent's own printed output, not an authoritative source.
- **Reads are inferred, not observed.** A filesystem watcher can't see a read; the Consulted list and the
  "read" badges depend entirely on the agent's own tool-output stream being parseable.
- **Radar is a hedge, not a proof.** "Activity overlap" can't distinguish two truly unrelated processes
  from the same agent revisiting its own file — an unresolved actor is always called out explicitly rather
  than asserted as a real conflict.
- **A multiply-edited path's replay diff isn't retained** at an earlier scrub point — only the latest
  content survives, and the drawer says so rather than guessing.
- **Checkpoint revert is a real filesystem write**, gated behind an explicit two-step confirm and a drift
  preview — it is not on the app's Ctrl+Z undo stack.
