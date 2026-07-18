---
id: CPE-683
title: Connections sidebar + OS-keychain credentials
type: feature
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-616
estimate: 3-4h
---

## Summary
Child of CPE-616. A "Connections" sidebar section to add/edit/remove a remote (host, auth, path), with
credentials stored in the OS keychain (never plaintext). Prereq: CPE-681. Needs GUI + keychain + attended.

## Acceptance Criteria
- [ ] Add/edit/remove a connection; it appears in the sidebar and navigates like a location.
- [ ] Credentials stored in the OS secure store; never written in plaintext.
- [ ] `npm run check` + suite green.

## Work Log
