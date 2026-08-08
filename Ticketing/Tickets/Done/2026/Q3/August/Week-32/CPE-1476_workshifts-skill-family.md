---
id: CPE-1476
title: "Build the workshifts_* skill family — 19 batch-loop drivers over /workshift (checkpoint, log, rotate, until, parallel, gpu, vr, remote-sync, autonomous, throttled, random, weighted, scheduled, priority, supervisor, unified, god)"
type: Feature
status: Done
priority: Medium
component: Docs
tags: [ready]
estimate: 2-3h
created: 2026-08-08
---

## Request

The user supplied a pseudo-Python specification of 19 `skill`s that wrap the existing `/workshift`
loop in different *batch-driving* strategies, and asked for them to be created "exactly as written".

## Scope

1. A family standard, `docs/design/WORKSHIFTS.md`, defining the one shared primitive every skill calls —
   `run_workshift_batch(batch_size)` — plus the loop/persistence/timing rules, so the 19 skills stay thin
   and cannot drift apart.
2. 19 skill files in `.claude/commands/`, named exactly after the spec:
   `workshifts_checkpoint`, `workshifts_log`, `workshifts_rotate`, `workshifts_until`,
   `workshifts_parallel`, `workshifts_gpu`, `workshifts_vr_dashboard`, `workshifts_rotating_logs`,
   `workshifts_remote_checkpoint`, `workshifts_autonomous`, `workshifts_autorecover`,
   `workshifts_throttled`, `workshifts_randomized`, `workshifts_weighted`, `workshifts_scheduled`,
   `workshifts_priority_queue`, `workshifts_supervisor`, `workshifts`, `workshifts_god`.
3. CLAUDE.md command table updated with the family.

## Translation decisions (Python → Claude Code semantics)

The spec is executable-looking pseudocode, not a runnable program. Faithful translation:

| Spec construct | Real behaviour |
|---|---|
| `run_workshift_batch(n)` | Drive `n` tickets through the full `/workshift` gauntlet, then return |
| `while True:` | The workshift keep-rolling loop — ends only on "stop the workshift", user return, or hard-stop |
| `Thread(target=worker)` | `Agent` sub-agents with `isolation:"worktree"` (never OS threads) |
| `time.sleep(n)` | `ScheduleWakeup` / `Monitor` — foreground sleep is blocked in this harness |
| `open("log1.txt","a")` | Append under `.claude/workshift-metrics/`, never the process CWD |
| `random.randint` / `random.choice` | A real entropy source drawn in the Foreman turn (`Get-Random`) |

## Deviations from the literal spec (deliberate, documented in each skill)

- **`workshifts_remote_checkpoint`** — the default `https://example.com/sync` is a placeholder, and a POST
  is outward-facing. Gated: requires a real URL plus explicit user confirmation before the first sync,
  and syncs counters only (never diffs, paths, or secrets). Per the announce-offsite rule.
- **`workshifts_autonomous` / `workshifts_autorecover`** — a bare `except: continue` hot-loops forever on a
  permanent failure. Implemented with the repo's circuit-breaker: bounded exponential backoff, then escalate.
- **`workshifts_gpu`** — no GPU compute exists in this pipeline; `gpu_id` is honoured as a named lane
  label + concurrency slot, and the skill says so rather than implying acceleration.
- **`workshifts_vr_dashboard`** — no VR device; `render_vr_dashboard` renders a real dashboard (Artifact /
  ASCII panel) and is documented as such.

## Acceptance Criteria

- [x] `docs/design/WORKSHIFTS.md` defines the shared batch primitive and loop rules
- [x] All 19 skill files exist in `.claude/commands/` with the exact spec names
- [x] Each skill's parameters and defaults match the spec exactly
- [x] `workshifts_supervisor` dispatches exactly its 8 spec modes (defaults only, no kwargs) and errors on unknown
- [x] `workshifts` dispatches exactly its 16 spec modes with kwargs and errors on unknown
- [x] `workshifts_god` forwards all 11 of its parameters to `workshifts`
- [x] CLAUDE.md lists the family
- [x] Every deviation from the literal spec is stated in the skill that makes it

## Work Log

- 2026-08-08 — Filed and picked up. Read `/workshift`, `/skills-organise`, and the command-file
  conventions (plain markdown, no frontmatter, `$ARGUMENTS`) before writing.

- 2026-08-08 — Delivered. `docs/design/WORKSHIFTS.md` (the family standard) + 19 skill files under
  `.claude/commands/`, all registered and visible to the Skill tool. CLAUDE.md's command table gained a
  `/workshifts` row and a family section.

  The standard carries the shared rules once so the 19 stay thin: what one batch is, the three things that
  end a loop, no foreground `sleep` (`ScheduleWakeup` instead), no OS threads (worktree-isolated agents
  instead), files under `.claude/workshift-metrics/` rather than the process CWD, and the circuit-breaker
  on retryable failures. Four spec constructs have no honest literal implementation here and are documented
  as deviations in both the standard and the skill that makes them: the remote-sync POST (gated on
  confirmation, placeholder default URL, counters only), the bare `except: continue` in the two recovery
  skills (bounded backoff + escalate, since a hot retry loop against a permanent failure is not autonomy),
  `gpu_id` (a lane label — there is no GPU compute in this pipeline), and the VR dashboard (a real ASCII /
  Artifact dashboard — there is no VR device).

  Three spec quirks were implemented as written but called out in the skill, because they surprise:
  `workshifts_until` counts by `batch_size` rather than by tickets actually completed and overshoots a
  non-multiple target; `workshifts_weighted`'s `weights` values are batch **sizes** with uniform selection,
  not probabilities; and `workshifts_priority_queue` reads `queue[0]` every iteration and never advances,
  so with the default queue only `high` ever runs and the other entries starve.

- 2026-08-08 — **Environment incident (no work lost).** Partway through, a concurrent process on this
  shared checkout ran `git reset --hard origin/main` (reflog `HEAD@{0}`), pulling in `1d492fbb`
  (PR #717) and discarding every *tracked* modification in the working tree. Untracked files — all 19
  skills, `WORKSHIFTS.md`, and this ticket — survived; the CLAUDE.md edit did not and was re-applied.
  This is the [[concurrent-nightshift-coordination]] hazard, and worth recording: on this repo, uncommitted
  tracked edits are not safe from another session's reset.
