---
id: CPE-977
title: "EPIC: AI file copilot — natural-language file operations"
type: Task
status: In Progress
priority: High
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-24
closed:
---

## Goal
Type an instruction in plain language — "move every PDF older than a year into Archive/2024", "rename these
photos to `Trip-###`", "delete the empty folders under here" — and the app turns it into a **previewed,
confirmable plan** of concrete file operations that reuse the existing safe primitives, then executes it on
approval with full undo. An AI copilot for the boring bulk work, with the human always in the loop.

## Why
This is where an AI explorer earns its name: not chat *about* files, but *doing* file work safely from
intent. The safety scaffolding already exists — `action_macro::plan` (filesystem-free op expansion),
`restore_plan` (revert), the [[CPE-729]] risk classifier + `needs_approval` gate, and the CPE-711
confirm-before-shelling-out pattern. The missing piece is the **NL → structured plan** translator plus a
tight preview/confirm/undo loop. High leverage, and it composes every batch capability already built
(rename, move, tag, convert).

## Rough scope (areas, not child tickets)
- A **structured operation plan** model (superset of/aligned with `action_macro::PlannedOp`): the typed,
  serialisable list an instruction compiles to — pure, validated, filesystem-free to build + preview.
- An **intent→plan translator** behind an `LlmPlanner` seam (pluggable model, feature-gated), producing the
  structured plan from NL + the current folder context; **never** free-form shell — only the whitelisted
  op vocabulary, so it's inspectable and safe.
- A **dry-run + diff preview**: show exactly what will change (reuse `restore_plan`/summary rendering),
  route every plan through the CPE-729 risk classifier, and **require confirmation** for destructive/
  high-impact steps.
- **Execute + undo**: run the plan via existing primitives (move/rename/delete/tag), recording a checkpoint
  ([[CPE-732]] snapshot store) so the whole operation is one-click reversible.

## Open questions (resolve at activation — big-design)
- **Planner backend:** which model/endpoint, local vs external, and the exact op vocabulary the model may
  emit (the tighter the safer). Prompt/JSON-schema contract for deterministic, validatable plans.
- Guardrails: hard caps (max files touched), always-confirm classes, path-scope confinement to the current
  tree, and how to surface low-confidence/ambiguous instructions rather than guessing destructively.
- Overlap with user-defined commands ([[CPE-711]]) and macros ([[CPE-739]]) — this is the NL front-end onto
  the same execution + confirm machinery, not a parallel one.

## Definition of Done
- A natural-language instruction produces a previewed, typed operation plan the user can confirm or cancel.
- Execution reuses the existing safe primitives, is risk-gated, and is fully undoable via a checkpoint.
- No plan step can run without confirmation where it is destructive or shells out; the plan vocabulary is a
  closed, inspectable set (no free-form command execution).

## Notes
- Build order: the **pure plan model + validator + dry-run/summary** first (headless, cargo-tested, reusing
  `action_macro`/`restore_plan`), then the pluggable `LlmPlanner` seam, then the preview/confirm UI. Leans on
  [[CPE-729]], [[CPE-732]], [[CPE-739]]. See [[avoid-modal-permission-popups]] for where consent controls
  live.

## Activation (2026-07-24, workshift Foreman — user away, decisions logged)
First slice = the **pure structured op-plan model + validator + dry-run** (CPE-990) in `cpe-server` (Rust),
reusing `action_macro`/`restore_plan` concepts — a typed, inspectable, filesystem-free plan the UI previews.
The **NL→plan translator** needs an LLM backend (user resource / big-design) → deferred + noted. Preview/
confirm/execute UI is attended.

### Child tickets
1. **CPE-990** — Pure `cpe-server::op_plan`: a typed `FileOpPlan` (whitelisted ops: move/rename/delete/
   mkdir/copy) + `validate` (path-scope confinement, caps, no free-form) + a dry-run `summary`. Cargo-tested.
   *Headless — buildable now.*
2. **CPE-992+** — the `LlmPlanner` seam (**needs a model backend — user resource**), risk-gate via CPE-729,
   preview/execute/undo via CPE-732. **Attended.**
