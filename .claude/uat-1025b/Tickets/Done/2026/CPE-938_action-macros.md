---
id: CPE-938
title: "Action-macro model: pure headless core for scriptable actions"
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-739
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary

First slice of epic CPE-739 (Scriptable actions / user macros): a pure, std-only, dependency-free model
for user macros in the Tauri-free `cpe-server` crate (`crates/server/src/action_macro.rs`).

A macro is a named sequence of steps (`Rename`/`Move`/`Tag`/`Convert`). `validate` rejects malformed
macros; `plan` expands a macro over a selection of input path strings into a flat, ordered, concrete list of
`PlannedOp`s — with **no filesystem access**, so it is fully unit-testable and reusable by the GUI, hotkey
bindings, or watched-folder rules later.

Public API:
- `enum MacroStep { Rename { template }, Move { dest }, Tag { label }, Convert { to_ext } }`
- `struct ActionMacro { name, steps }` (Debug, Clone, PartialEq, Serialize)
- `struct PlannedOp { input, kind, detail }` (Serialize)
- `fn validate(&ActionMacro) -> Result<(), String>`
- `fn plan(&ActionMacro, &[String]) -> Vec<PlannedOp>`

Rename templates support `{name}`, `{stem}`, `{ext}`, and `{n}` (1-based selection index). `plan` orders
inputs-outer / steps-inner deterministically. Windows and POSIX paths and dotfiles are handled.

## Acceptance Criteria

- [x] New module `crates/server/src/action_macro.rs`, std-only, no new deps.
- [x] `MacroStep`, `ActionMacro` (derives Debug/Clone/PartialEq/Serialize), `PlannedOp`.
- [x] `validate` rejects empty name, empty steps, unknown rename tokens, empty move dest / tag label /
      convert ext.
- [x] `plan` is pure (no fs), expands rename templates incl. `{n}` index, deterministic ordering.
- [x] Thorough `#[cfg(test)]` tests (14 passing) covering validate (ok + each rejection) and plan.
- [x] Module registered via `pub mod action_macro;` in `crates/server/src/lib.rs`.
- [x] `cargo test action_macro` green; `cargo clippy --all-targets -- -D warnings` clean.

## Work Log

- 2026-07-23: Authored `action_macro.rs` (model + `validate` + `plan` + helpers) and registered the module
  in `lib.rs`. 14 tests pass; clippy clean after switching a manual char comparison to an array pattern.
  Activated epic CPE-739 (this is its first slice). Branch `cpe-938-action-macros`, PR opened (no merge —
  orchestrator serializes merges).
