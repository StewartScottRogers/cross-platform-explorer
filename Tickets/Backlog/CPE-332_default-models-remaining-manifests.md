---
id: CPE-332
title: "Populate default models for the remaining agent manifests (from reference)"
type: Task
status: Open
priority: Medium
component: Backend
created: 2026-07-13
---

## Summary

CPE-328 added per-provider default-model support and populated `claude.json`. Do the same
for the other bundled agents using the reference's per-agent defaults (e.g. Codex native
`gpt-5.5` / openrouter `openai/gpt-5.1-codex`; read each `<Agent>--openrouter.cmd` /
`--is-installed.cmd` in AgenticCliOptions). Also mirror the reference's per-agent PROVIDER
SUPPORT: Gemini/Grok/AmazonQ/Codebuff have no openrouter — our manifests should not offer a
provider the agent can't use.

## Acceptance
- Every bundled agent launches its providers with sensible default models (only secrets
  required from the user), and only advertises providers it actually supports.
