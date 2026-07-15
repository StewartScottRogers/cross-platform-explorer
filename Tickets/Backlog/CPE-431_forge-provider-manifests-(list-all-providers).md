---
id: CPE-431
title: "Forge provider manifests (list all providers)"
type: Feature
status: Open
priority: High
component: Backend
tags: [ready]
estimate: 1h
created: 2026-07-15
epic: CPE-429
---

## Summary
The list-all-of-them data (CPE-429): bundled provider manifests for the major forges/VCS, loaded by the
CPE-430 registry. Tier-1 git forges first; others as data follow-ups.

## Acceptance Criteria
- [ ] Bundled manifests: github, github-enterprise, gitlab, bitbucket, gitea, forgejo, codeberg,
      sourcehut, azure-devops, aws-codecommit, generic-git.
- [ ] Each declares kind=git, auth (oauth/pat/ssh/anonymous), api_hosts, capabilities.
- [ ] A catalog test loads them all cleanly and asserts each capability is coherent.
- [ ] Non-git (mercurial/subversion/perforce) stubbed as experimental follow-up manifests.
