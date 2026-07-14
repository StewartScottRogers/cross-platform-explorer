---
id: CPE-387
title: "Test: a key saved via the Keys panel is the one the launch flow uses"
type: Test
status: Done
priority: Low
component: Backend
tags: [ready]
created: 2026-07-14
closed: 2026-07-14
---

## Summary

The key-handling flow was covered piecewise (resolution precedence, save/list/delete + no-leak,
verify, key→agent-env injection in scope.rs). Added a cohesive end-to-end test proving the
`provider_secret_name` contract holds between the Keys panel API (`handle_key_set`) and the launch
resolver (`resolve_provider_key`) — the "GUI can use this key" path — including default vs labelled
credentials, typed-key precedence, and that deleting the default leaves a labelled key intact.

## Acceptance Criteria
- [x] Save via `/api/keys` → `resolve_provider_key` returns exactly that key (default + label).
- [x] Typed key overrides the stored one; delete default → resolves to none; label untouched.
- [x] Fake keys only. 125 ai-console tests; clippy --all-targets -D warnings clean.

## Work Log
2026-07-14 — Added after live-verifying a real OpenRouter key (not stored anywhere).
