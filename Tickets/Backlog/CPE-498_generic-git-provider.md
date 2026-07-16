---
id: CPE-498
title: "Generic Git provider — clone/sync any HTTPS/SSH remote + self-hosted host admission"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [ready]
estimate: 3-4h
created: 2026-07-16
epic: CPE-488
---

## Summary
Provider priority (Q4): the next provider is **Generic Git** — clone/sync **any** HTTPS/SSH git remote
regardless of a known forge API, which also covers **self-hosted** instances. Includes per-connection
**egress host admission** (Q5): a user-supplied host is added to the allow-list only with **explicit
consent**, never a wildcard, so self-hosted works without opening an SSRF hole.

## Acceptance Criteria
- [ ] Clone + two-way sync work against an arbitrary HTTPS/SSH git URL via a generic-git path.
- [ ] A user-supplied host is admitted to the host-brokered egress allow-list only after explicit
      per-connection consent (no wildcard, no silent admission).
- [ ] Browse degrades gracefully where there is no forge API (clone/sync still work).
- [ ] Credentials (token / SSH) via the secrets broker, per connection.
- [ ] Tests for URL/host parsing + the consent-gated admission.

## Notes
Ties Q4 (Generic Git) + Q5 (self-hosted egress). Other forges (GitLab/Bitbucket/Gitea) are follow-up
manifests, out of this v2 wave.
