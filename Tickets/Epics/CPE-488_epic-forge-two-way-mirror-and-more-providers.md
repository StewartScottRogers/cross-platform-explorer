---
id: CPE-488
title: "EPIC: Forge v2 — two-way mirror UI + more providers (self-hosted & non-Git)"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-16
closed:
---

## Summary
The post-v1 continuation of the forge Mega-Feature ([[CPE-429]], closed as v1-delivered): finish the
**two-way mirror** experience and broaden **provider** coverage beyond "browse + clone a GitHub repo".
This is a **brief only** — the goal and rough scope are captured here; it is not decomposed into child
tickets until it is activated (`/ticketing-epic activate CPE-488`).

## Goal
Take forge integration from "browse any GitHub/forge repo and clone it" (v1, native) to "keep a local
mirror and its remote in **two-way sync**, across **many** forges and VCS backends" — the original
CPE-429 north star, minus what v1 already shipped.

## Rough scope (NOT decomposed — for sizing only)
- **Mirror/sync UI.** The sync *engine* / planner already exists ([[CPE-438]], Done). What's missing is
  the surface: wire **push** + a divergence/conflict view into the Repositories pane, with dry-run /
  preview, per-repo sync policy, and a safe-by-default conflict path (never silent force-push).
- **More provider parsers.** v1 covers the GitHub Contents API. Add the other Tier-1 forges as
  manifest + parser: GitLab, Bitbucket, Gitea/Forgejo, Codeberg, SourceHut, Azure DevOps,
  AWS CodeCommit, and the universal **Generic Git** HTTPS/SSH fallback.
- **Self-hosted forges.** Connect to self-hosted GitLab / Gitea / Bitbucket Data Center instances
  (custom base URL → the host-brokered egress allow-list must admit per-connection hosts safely).
- **Tier 2/3 VCS (stretch).** Mercurial / Subversion / Perforce / Fossil, and Radicle — best-effort or
  explicit non-goals; decide at activation.
- **Isolation revisit.** Whether any of the above finally justifies moving forge into the **repos
  sidecar** ([[CPE-432]], now a bundled/registered/conformant tenant) instead of the native path.

## Open questions (resolve at activation, with the user)
- **Q1 — Mirror conflict model:** how much conflict UI is v2 (three-way? just "diverged, choose pull /
  push / open in tool"?) vs. deferring hard conflicts to an external merge tool.
- **Q2 — Auto vs manual sync:** on-demand only, or scheduled/background mirroring? If background, how is
  it surfaced and paused?
- **Q3 — Native path vs repos sidecar:** keep extending the native `forge_*` host commands, or migrate
  to the `repos` sidecar for process isolation of untrusted-repo operations? (Ties to [[CPE-432]].)
- **Q4 — Provider priority:** which forges after GitHub actually matter to users — pick the first 2–3
  to build rather than all at once.
- **Q5 — Self-hosted egress safety:** how a user-supplied base URL is admitted to the allow-list
  without opening an SSRF hole (per-connection host pinning, explicit consent).
- **Q6 — Tier 2/3 VCS:** in scope for v2 or explicit non-goals?

## Definition of Done (epic-level — refined at activation)
- [ ] Two-way mirror is usable end-to-end for at least Generic Git + GitHub: pull **and** push with a
      clear divergence/conflict surface; never silently loses work.
- [ ] At least the top few Tier-1 providers (chosen in Q4) browse + clone via manifest parsers.
- [ ] Self-hosted instances connect within the host-brokered, allow-listed egress model (no SSRF).
- [ ] Q3 decided and recorded (native vs sidecar), with the threat model ([[CPE-440]]) extended to any
      new egress hosts and to push/write operations.
- [ ] Child tickets all Done; conformance/threat gates green.

## Notes
Successor to [[CPE-429]]. Builds on shipped pieces: [[CPE-433]] (host-brokered egress), [[CPE-434]]
(GitHub browse), [[CPE-435]] (Repositories pane), [[CPE-436]] (clone), [[CPE-438]] (mirror engine),
[[CPE-439]] (credentials), [[CPE-440]] (threat model), and [[CPE-432]] (repos sidecar tenant). Filed
as a dormant brief on 2026-07-16 per the just-in-time epic workflow ([[CPE-487]]); activate to research,
resolve Q1–Q6 with the user, and split into child tickets.
