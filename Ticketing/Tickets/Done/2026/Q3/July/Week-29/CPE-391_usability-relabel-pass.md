---
id: CPE-391
title: "AI Console: de-jargon the labels for inexperienced users"
type: Task
status: Done
priority: Medium
component: Frontend
tags: [ready]
created: 2026-07-14
closed: 2026-07-14
---

## Summary

Usability pass on the AI Console labels for newcomers (user feedback). Renamed the genuinely-jargon
controls and added plain-language tooltips to the recognizable-but-technical ones; ids unchanged so
all wiring/tests hold.

- **Preset → Setup** ("Save setup"; "— saved setups —"; "Name this setup…").
- **Credential → Account.**
- **Small model → Fast model.**
- Working folder → **Project folder**; ⇕ Output → **⇕ Full output**.
- Tooltips added on Agent / Provider / Model / API key (plain-language "what is this").
- Help panel text updated to match.

## Acceptance Criteria
- [x] Jargon labels renamed; tooltips on technical fields; Help panel consistent.
- [x] 8 launcher harness tests pass (target ids/behaviour, not labels); ai-console builds; JS OK.

## Work Log
2026-07-14 — Applied recommended set (Saved setup / Account) after the naming question.
