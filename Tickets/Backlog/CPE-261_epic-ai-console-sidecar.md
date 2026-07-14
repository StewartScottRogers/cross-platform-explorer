---
id: CPE-261
title: "EPIC: AI Console sidecar (agentic CLI manager)"
type: Task
status: Open
priority: High
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-13
---

## Summary

The first Mega-Feature sidecar: an **agentic CLI manager + embedded terminal**.
Install/manage *any* coding-agent CLI with *any* provider and *any* model, run the
native CLI in an in-app console scoped to the open repo, extend agents with plugins,
and keep multi-environment credentials secure — **management logic in Rust, no shell
scripts**, everything **CLI-agnostic and manifest-extensible**. Modelled on
`Z:\repos\AgenticCliOptions` (agent lifecycle × provider routing × plugin fan-out),
ported to Rust and hardened. Built strictly as a **sidecar tenant** of the platform
([[CPE-260]]); obeys the charter [[CPE-259]].

## Child tickets

**Core**
- [[CPE-277]] AI Console sidecar skeleton
- [[CPE-278]] Agent registry + agent manifest schema
- [[CPE-279]] Secret vault + credential profiles
- [[CPE-280]] Embedded PTY console — run an installed agent (MVP)
- [[CPE-281]] Lifecycle: detect / is-installed
- [[CPE-282]] Lifecycle: install / update
- [[CPE-283]] Lifecycle: uninstall
- [[CPE-284]] Aggregate ops (install/update/uninstall all)
- [[CPE-285]] Provider/model routing engine
- [[CPE-286]] LM Studio auto-detection
- [[CPE-287]] Provider credential UI + key verification
- [[CPE-288]] Plugin/extension system (MCP fan-out)
- [[CPE-289]] Agent session launcher UI
- [[CPE-290]] Multi-agent sessions / tabs
- [[CPE-291]] Seed initial agent catalog (~20 agents)
- [[CPE-292]] Session persistence & history
- [[CPE-293]] "Add a new agent" extensibility guide

**Cross-cutting (added in end-to-end hardening)**
- [[CPE-306]] Agent process scoping & execution sandbox
- [[CPE-307]] MCP server lifecycle & credentials
- [[CPE-308]] Agent catalog update / subscription
- [[CPE-309]] Session reattachment across sidecar restart
- [[CPE-310]] Enterprise networking: proxy, offline & air-gapped
- [[CPE-311]] Usage/cost tracking & opt-in telemetry
- [[CPE-312]] First-run onboarding
- [[CPE-313]] Task/prompt injection from explorer context
- [[CPE-305]] Console ↔ Agent Watch integration
- (shared) [[CPE-314]] Accessibility & i18n

## Schedule (dependency-ordered waves) — starts after Platform P4

- **C1 — Foundation:** [[CPE-277]], [[CPE-278]], [[CPE-279]], [[CPE-306]] (design).
- **C2 — Console MVP:** [[CPE-281]], [[CPE-280]], [[CPE-306]] (enforce), [[CPE-309]],
  [[CPE-313]] (basic). *Exit: a real `claude`/`aider` session runs in-app, scoped +
  reattachable.*
- **C3 — Lifecycle:** [[CPE-282]], [[CPE-283]], [[CPE-284]], [[CPE-310]].
- **C4 — Providers:** [[CPE-285]], [[CPE-286]], [[CPE-287]].
- **C5 — UX & extension:** [[CPE-289]], [[CPE-288]], [[CPE-307]], [[CPE-290]],
  [[CPE-305]], [[CPE-313]] (rich).
- **C6 — Catalog & polish:** [[CPE-291]], [[CPE-308]], [[CPE-292]], [[CPE-311]],
  [[CPE-312]], [[CPE-314]], [[CPE-293]].

**Depends on:** [[CPE-260]] (through P4), [[CPE-259]]. Security-reviewed per
[[CPE-304]].

## Definition of Done (epic-level gates)

- [ ] All child tickets Done; conformance kit ([[CPE-301]]) + AI-Console E2E green.
- [ ] Any agent × any provider × any model launchable; adding an agent/provider/
      plugin is **manifest-only**, no code change (proven by [[CPE-293]] worked example).
- [ ] Secrets never in plaintext at rest, never logged, never in the webview; a
      launched agent runs scoped with disclosed trust ([[CPE-306]]).
- [ ] Runs isolated as a sidecar; explorer builds/ships without it (delete-test).
- [ ] Works behind a proxy and degrades cleanly offline.

## Key risks

- **Arbitrary-code/credential exposure** via agents & manifests — mitigated by
  [[CPE-295]], [[CPE-296]], [[CPE-306]], [[CPE-304]].
- **PTY/TUI fidelity across OSes** — de-risked by [[CPE-294]].
- **Market drift of install recipes** — absorbed by manifest catalog + [[CPE-308]].
- **Session loss on restart/update** — handled by [[CPE-309]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Hardened: added agent sandbox/scoping, MCP-server lifecycle, catalog
updates, session reattachment, enterprise networking, cost/telemetry, onboarding,
explorer→console task injection, and Agent Watch integration. Reworked waves and
added epic-level DoD + risks.
