# workshifts_vr_dashboard — Batch Loop with a Live Dashboard

Run `/workshift` in repeated batches, **rendering a dashboard after every batch** so an away user can see
shift state at a glance instead of reading back through the transcript.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## What "VR" means here — read this first

**There is no VR device in this environment and this skill does not drive one.** `render_vr_dashboard(msg)`
renders a real, viewable dashboard:

- **Default — an ASCII panel in the transcript.** Zero setup, readable from across the room, and consistent
  with this repo's ASCII-banner convention ([[use-ascii-art-when-addressing-user]]).
- **Richer — a published Artifact page** when the user wants a persistent, refreshable view. Same file path
  each redeploy so the URL is stable, and it is only published when asked for.

The name is kept because the spec names it. The behaviour is stated honestly rather than dressed up as
headset output.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `batch_size` | `10` | Tickets driven through the gauntlet per batch |

## Procedure

Loop until a family stop condition fires:

1. `run_workshift_batch(batch_size)`.
2. `render_vr_dashboard("Completed batch of <batch_size>")` — redraw the dashboard.
3. Report `[VR] Restarting...`.

## What the dashboard shows

A counter alone is not a dashboard. Each redraw carries:

- **Wall-clock time** and the next-wake time ([[loop-behavior-needs-timestamps]]).
- **This batch** — tickets merged (with IDs and titles in plain language), failed, still in flight.
- **Cumulative** — batches completed, total tickets merged since the run started.
- **Queue depth** — ready / blocked / deferred counts, so a drying queue is visible before it bites.
- **Health** — CI status on `main`, open PR count, sub-agent budget remaining.

Written for someone who has been away for hours: names and outcomes, not ticket IDs and jargon
([[workshift-summarize-with-context]]).

## Notes

- The dashboard **replaces** the per-batch report line; it does not stack on top of it. One redraw per batch.
- Rendering must never steal focus or open a window over the user's screen
  ([[automation-must-not-hijack-screen]]).
