---
id: CPE-458
title: "Document Repositories + AI model features in the README"
type: Task
status: Done
priority: Low
component: Docs
tags: [ready]
created: 2026-07-15
closed: 2026-07-15
---

## Summary
The sidecar build gained substantial user-facing features this session (Repositories browse/clone,
the inline any-model dropdown, per-reseller keys, console close menus) but the README didn't mention
them. Add a Features section so they're discoverable.

## Acceptance Criteria
- [x] README has a Features section covering Repositories, AI Console/Agent Watch, and any-model selection.
- [x] Notes these are sidecar-build-gated; the plain explorer stays fast/small by default.

## Resolution
Added a "Features" section near the top of `README.md` summarizing the Repositories in-app
browse/clone (host-brokered, hardened clone, keychain token), the AI Console + Agent Watch + close
menus, and the inline any-reseller Model dropdown with keychain keys — with the sidecar-gating caveat.
Docs-only.
