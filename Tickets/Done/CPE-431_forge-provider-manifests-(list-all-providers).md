---
id: CPE-431
title: "Forge provider manifests (list all providers)"
type: Feature
status: Done
priority: High
component: Backend
tags: [ready]
estimate: 1h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-429
---

## Summary
The list-all-of-them data (CPE-429): bundled provider manifests for the major forges/VCS, loaded by the
CPE-430 registry. Tier-1 git forges first; others as data follow-ups.

## Acceptance Criteria
- [x] Bundled manifests: github, github-enterprise, gitlab, bitbucket, gitea, forgejo, codeberg,
      sourcehut, azure-devops, aws-codecommit, generic-git.
- [x] Each declares kind=git, auth (oauth/pat/ssh/anonymous), api_hosts, capabilities.
- [x] A catalog test loads them all cleanly and asserts each capability is coherent.
- [x] Non-git (mercurial/subversion/perforce) stubbed as experimental follow-up manifests.

## Work Log
2026-07-15 - Nightshift. Added 14 bundled provider manifests under sidecar/repos/providers/: github, github-enterprise, gitlab, bitbucket, gitea, forgejo, codeberg, sourcehut, azure-devops, aws-codecommit, generic-git (git); mercurial, subversion, perforce (non-git, experimental). Each declares kind/auth/api_hosts/capabilities/web_base. Catalog test loads them cleanly, asserts coherence (every provider can clone; non-git flagged experimental) and that known API hosts land in the egress allow-list. 3 catalog tests, clippy clean.
