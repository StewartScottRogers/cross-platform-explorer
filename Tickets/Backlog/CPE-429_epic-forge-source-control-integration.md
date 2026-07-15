---
id: CPE-429
title: "EPIC: Source-control / forge integration — browse & two-way mirror any repository, as sidecars"
type: Task
status: Open
priority: High
component: Multiple
tags: [epic, big-design, needs-decision]
estimate: 4h+
created: 2026-07-15
---

## Summary

A new Mega-Feature, peer to the AI Console ([[CPE-261]]) and built as a **sidecar tenant** of the
platform ([[CPE-260]], governed by ADR 0001 / [[CPE-259]]): connect to and interact with **any**
source-code repository on the internet, and keep a **local copy and its remote in two-way sync**.

The user's goal, restated:

- A dedicated **left-pane section** ("Repositories" / "Sources") — its own area in the explorer
  sidebar, distinct from Quick Access, drives, and Agent Watch.
- **Local repos** — the user's own projects on this machine, discovered and managed.
- **Remote repos** — browse *any* accessible repository: public GitHub browse-anything, plus
  authenticated private repos across providers.
- **Two-way mirroring / interconnect** — a local mirror stays in sync with its remote in **both**
  directions (pull + push), with divergence/conflict handling — not a one-shot clone.
- **Provider-agnostic** — the same capability for **every** forge/VCS, via **manifest-extensible
  providers** (like the AI Console is CLI-agnostic). Adding a provider is data, not host code.
- **Sidecar-based** — secrets (tokens/PATs/SSH keys) via the secrets broker ([[CPE-268]]); all
  network egress **host-brokered + allow-listed** per provider (extending §7 of the threat model,
  [[CPE-347]]/[[CPE-369]]/[[CPE-376]]); one-way dependency, process isolation, consent-gated caps.

## Providers to support (the epic "lists all of them")

**Tier 1 — Git forges (API + git):**
- GitHub (github.com) + GitHub Enterprise Server
- GitLab (gitlab.com + self-hosted)
- Bitbucket Cloud + Bitbucket Data Center/Server
- Gitea + Forgejo (self-hosted)
- Codeberg (Forgejo instance)
- SourceHut (git.sr.ht)
- Azure DevOps (Repos)
- AWS CodeCommit
- Google Cloud Source Repositories
- Gerrit
- **Generic Git** — any HTTPS/SSH remote (self-hosted, cgit/gitweb), the universal fallback

**Tier 2 — other VCS (own protocols):**
- Mercurial (hg — SourceForge, Heptapod)
- Subversion (SVN — Apache, self-hosted)
- Perforce Helix (p4)
- Fossil
- Bazaar (bzr) / CVS — legacy, best-effort or explicit non-goals

**Tier 3 — decentralized / novel:**
- Radicle (P2P git)

The provider set is **data** (one manifest per provider): capabilities (browse/clone/push/pull),
auth model (OAuth / PAT / SSH / anonymous), API base URL(s) for allow-listed egress, and the VCS
backend it drives. Tier-1 lands first; the rest slot in as manifests.

## Architecture (grounded in ADR 0001)

- **`repos` sidecar tenant** — a standalone process behind the sidecar contract. Provider-agnostic
  core (browse / clone / status / sync) + a **provider registry** loaded from manifests (mirrors the
  AI Console's agent registry, [[CPE-278]]).
- **Two-way sync engine** — pull + push with fast-forward/merge/rebase policy, divergence detection,
  and a clear conflict surface; never silently loses work.
- **Left-pane section** — the explorer surfaces the sidecar's repositories in a dedicated sidebar
  section (like Agent Watch surfaces sessions), with connect/disconnect, clone, sync, and status.
- **Auth & egress** — tokens/keys in the OS keychain via the secrets broker ([[CPE-268]]); the host
  performs allow-listed API calls on the sidecar's behalf (no SSRF), per provider manifest.
- **Trust** — extends the CPE-304 threat model: new egress hosts (one section per provider), token
  handling, and the fact that cloning/pulling brings **untrusted content** to disk (scan/consent).

## Proposed child tickets (to be split after design sign-off)

**Foundation**
- Provider contract + manifest schema (capabilities, auth, egress hosts, VCS backend)
- `repos` sidecar skeleton (handshake, registry, host-brokered egress)
- Credentials: OAuth/PAT/SSH via the secrets broker; per-provider login flows
- Left-pane "Repositories" section (connect / list / status) in the explorer

**Browse & clone**
- Remote browse (read-only tree/file via provider API) — public + authenticated
- Clone a remote repo to a local path

**Mirror / sync (the two-way core)**
- Local repo discovery + status (ahead/behind/dirty)
- Sync engine: pull, push, divergence + conflict handling, dry-run/preview
- Scheduled / on-demand mirroring; per-repo sync policy

**Providers (data)**
- GitHub · GitLab · Bitbucket · Gitea/Forgejo · Codeberg · SourceHut · Azure DevOps · AWS CodeCommit
  · Generic Git · (Tier 2/3 as follow-ups)

**Hardening**
- Threat-model section: token safety, allow-listed egress per provider, untrusted-clone consent
- Offline / enterprise-proxy behaviour (reuse [[CPE-310]])

## Open questions (need-decision — for sign-off before building)

- **D1 — Scope of "interact":** browse + clone + two-way mirror is the core. Do we also want
  issues/PRs/CI status (forge API surface), or is that out of scope v1?
- **D2 — VCS backend:** shell out to installed `git`/`hg`/`svn` (like the AI Console shells to
  agent CLIs) vs. an embedded library (gitoxide/libgit2)? Shelling out is CLI-agnostic + consistent
  with ADR 0001; embedding avoids a dependency on installed tooling. **Recommend: shell out, with
  the tool declared per provider manifest.**
- **D3 — Sync conflict policy:** default to safe (never auto-force-push; surface conflicts for the
  user) vs. configurable per repo. **Recommend: safe-by-default + per-repo override.**
- **D4 — One `repos` sidecar with provider plugins, or one sidecar per provider?** **Recommend: one
  provider-agnostic `repos` sidecar with manifest providers** (matches the AI Console pattern; a
  provider is data, not a new process).
- **D5 — Non-Git VCS (Tier 2/3):** first-class or explicit non-goals for v1? **Recommend: Git-first;
  the generic-Git + top forges in v1, others as manifest follow-ups.**

## Notes
Modeled on the AI Console's "any CLI × any provider × any model" success ([[CPE-261]]): here it's
"any forge × any repo × two-way sync". Depends on the sidecar platform ([[CPE-260]]) and the secrets
broker ([[CPE-268]]); applies the threat-model discipline of [[CPE-304]]. Filed during the Nightshift
from a direct user request; **held for design sign-off (D1–D5) before splitting into ready slices** —
this is a Mega-Feature the size of the AI Console.

## Work Log
2026-07-15 — Filed from the user's request for GitHub + all-forge integration (two-way mirroring, a
left-pane section, sidecar-based). Restated the intent, enumerated providers, sketched the sidecar
architecture + child tickets, and captured the open design questions. Awaiting sign-off on D1–D5.

## Decision (2026-07-15, dayshift) — native-first forge, sidecar deferred
The forge integration shipped as a **native explorer feature** for v1, not the isolated sidecar
tenant this epic (and ADR 0001 / D2) originally assumed:
- **Host commands** (feature-gated): `forge_browse` (CPE-434), `forge_clone` (CPE-436),
  `forge_set/get/delete_token` (CPE-439), backed by `src-tauri/src/forge_egress.rs` (CPE-433 —
  allow-listed, no SSRF; the sidecar never supplies a URL, the host builds it).
- **UI**: a **Repositories** entry in the explorer left pane → `RepoBrowser.svelte` (CPE-435):
  browse any GitHub/forge repo tree, clone to a chosen folder, remember the token in the keychain.

**Why:** it delivered the visible, usable feature (see + browse + clone GitHub) far faster than
standing up a whole hosted sidecar + iframe pane. The provider allow-list, clone hardening
(threat-model §C), and credential handling all live in the native path.

**What this means for the children:**
- CPE-433 (egress), 434 (browse), 436 (clone), 439 (credentials), 435 (left-pane) — **DONE** natively.
- CPE-432 (repos sidecar process skeleton) — **Deferred**: superseded for v1; revisit if forge needs
  process isolation or the long-lived two-way **mirror** engine (CPE-438 planner is built; push/mirror
  UI is not wired).
- CPE-440 (threat model) — applies to the native path; ADR sign-off still wants a GUI run.

**Still open under the epic:** two-way **mirror/sync** (push + the CPE-438 planner wired to a UI),
more provider parsers beyond GitHub Contents, and self-hosted-forge connections. Revisit the
sidecar architecture (CPE-432) only if isolation/mirror needs demand it.
