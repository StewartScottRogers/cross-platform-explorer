---
id: CPE-261
title: "EPIC: AI Console sidecar (agentic CLI manager)"
type: Task
status: Open
priority: High
component: Multiple
estimate: 4h+
created: 2026-07-13
---

## Summary

The first Mega-Feature sidecar: an **agentic CLI manager + embedded terminal**.
Install/manage *any* coding-agent CLI with *any* provider and *any* model, run the
native CLI in an in-app console scoped to the open repo, extend agents with
plugins, and keep multi-environment credentials secure — all with **management
logic in Rust, no shell scripts**, and the whole thing **CLI-agnostic and
extensible by manifest**. Modelled on the patterns in `Z:\repos\AgenticCliOptions`
(agent lifecycle × provider routing × plugin fan-out), ported to Rust and hardened.

Built strictly as a **sidecar tenant** of the platform ([[CPE-260]]); obeys the
charter [[CPE-259]]. No entanglement with the explorer.

## Child tickets

- [[CPE-277]] AI Console sidecar skeleton (contract impl + empty pane)
- [[CPE-278]] Agent registry + agent manifest schema
- [[CPE-279]] Secret vault + credential profiles
- [[CPE-280]] Embedded PTY console — run an installed agent (MVP)
- [[CPE-281]] Lifecycle: detect / is-installed (per-OS)
- [[CPE-282]] Lifecycle: install / update (winget/npm/pipx/brew, Rust-orchestrated)
- [[CPE-283]] Lifecycle: uninstall
- [[CPE-284]] Aggregate ops (install / update / uninstall all)
- [[CPE-285]] Provider / model routing engine (env recipes)
- [[CPE-286]] LM Studio auto-detection (LAN probe) in Rust
- [[CPE-287]] Provider credential UI + key verification
- [[CPE-288]] Plugin / extension system (MCP fan-out)
- [[CPE-289]] Agent session launcher UI (agent × provider × model)
- [[CPE-290]] Multi-agent sessions / tabs
- [[CPE-291]] Seed initial agent catalog (~20 agents)
- [[CPE-292]] Session persistence & history
- [[CPE-293]] "Add a new agent" extensibility guide (docs)

## Schedule (dependency-ordered waves)

Starts only after Platform **P4** (CPE-260 MVP).

- **C1 — Foundation:** CPE-277 → CPE-278, CPE-279
- **C2 — Console MVP:** CPE-281, then CPE-280 (run an already-installed agent in
  the open repo). *Exit criterion: a real `claude`/`aider` session runs in-app.*
- **C3 — Lifecycle:** CPE-282, CPE-283, CPE-284
- **C4 — Providers:** CPE-285, CPE-286, CPE-287
- **C5 — UX & extension:** CPE-289, CPE-288, CPE-290
- **C6 — Catalog & polish:** CPE-291, CPE-292, CPE-293

**Depends on:** [[CPE-260]] (through P4), [[CPE-259]].

## Acceptance Criteria

- [ ] All child tickets Done.
- [ ] Any agent × any provider × any model launchable from the console; adding a
      new agent/provider/plugin is manifest-only, no code change.
- [ ] Secrets never stored in plaintext, never logged, never in the webview.
- [ ] Runs isolated as a sidecar; explorer builds/ships without it.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
