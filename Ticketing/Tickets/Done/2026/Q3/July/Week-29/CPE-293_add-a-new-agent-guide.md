---
id: CPE-293
title: "\"Add a new agent\" extensibility guide (docs)"
type: Task
status: Done
priority: Medium
component: Docs
estimate: 1-2h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Document how anyone adds a new coding-agent CLI, provider, model, or plugin **by
manifest, without code** — the promise of "CLI-agnostic and extensible." Turns the
schemas from [[CPE-278]]/[[CPE-285]]/[[CPE-288]] into a copy-paste-and-edit guide.

## Acceptance Criteria

- [ ] Step-by-step: add an agent manifest (detect/install/run/providers), drop it
      in the user registry, see it appear — no rebuild.
- [ ] How to add a provider routing recipe and a plugin manifest.
- [ ] A worked example (one new agent end-to-end).
- [ ] Linked from README/CLAUDE.md and the AI Console UI ("add your own").

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]], [[CPE-285]], [[CPE-288]]. **Phase:** C6.
**Epic:** [[CPE-261]].

## Resolution

Wrote `sidecar/ai-console/docs/adding-an-agent.md`: a step-by-step guide to adding an agent, a provider recipe, and a plugin **by manifest, no code** — with the full agent-manifest example, the provider-recipe placeholder syntax ({model}/{api_key}/…), the plugin manifest, a worked end-to-end example, and how to validate (skip-on-error diagnostics + the bundled catalog as copyable examples). Linked from `sidecar/README.md`. Reflects the schemas from [[CPE-278]]/[[CPE-285]]/[[CPE-288]].

**Note:** the 'link from the AI Console UI ("add your own")' bullet lands with the UI ([[CPE-289]]).

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Wrote the extensibility guide during dayshift. Done.
