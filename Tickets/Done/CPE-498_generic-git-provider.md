---
id: CPE-498
title: "Generic Git provider — clone/sync any HTTPS/SSH remote + self-hosted host admission"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
estimate: 3-4h
created: 2026-07-16
epic: CPE-488
closed: 2026-07-16
---

## Summary
Provider priority (Q4): the next provider is **Generic Git** — clone/sync **any** HTTPS/SSH git remote
regardless of a known forge API, which also covers **self-hosted** instances. Includes per-connection
**egress host admission** (Q5): a user-supplied host is added to the allow-list only with **explicit
consent**, never a wildcard, so self-hosted works without opening an SSRF hole.

## Acceptance Criteria
- [x] Clone + two-way sync work against an arbitrary HTTPS/SSH git URL via a generic-git path.
- [x] A user-supplied host is admitted to the host-brokered egress allow-list only after explicit
      per-connection consent (no wildcard, no silent admission).
- [x] Browse degrades gracefully where there is no forge API (clone/sync still work).
- [x] Credentials (token / SSH) via the secrets broker, per connection.
- [x] Tests for URL/host parsing + the consent-gated admission.

## Resolution
Added a **Generic Git** provider that clones/syncs any HTTPS/SSH remote (incl. self-hosted), gated on
consent-based host admission.

- **`sidecar/repos/src/generic.rs` (new, pure + tested):** `parse_remote(url)` extracts the bare
  **host** (the allow-list identity) and a **credential-stripped canonical URL** from `https://`,
  `ssh://`, and scp-like `user@host:path` remotes; rejects every other transport (`git://`/`file://`/
  `ext::`/`http://`), strips userinfo, keeps the port, and normalizes the host (lowercase, trailing-dot).
  8 unit tests. Exported from the crate.
- **Backend (`src-tauri/src/lib.rs`, `sidecar-platform`):**
  - Egress allow-list persisted at `app_data_dir/forge-admitted-hosts.json`; loads **fail-closed**
    (unreadable ⇒ admits nothing).
  - `forge_generic_remote(url)` → `{ host, scheme, url, admitted }` for the consent prompt (read-only).
  - `forge_admit_host` / `forge_forget_host` / `forge_admitted_hosts` — admission stores **exactly the
    normalized host**, refusing wildcards/paths (consent to `a.example.com` never admits `b.example.com`).
  - `build_generic_clone` (pure) refuses a non-admitted host, injects an https token as userinfo (ssh
    uses the agent — token ignored), and defers to the repos crate's hardened clone builder;
    `forge_clone_url` runs it, scrubbing the token from any error. 5 unit tests.
  - **Sync** needs no new code: a generic remote, once cloned, syncs through the existing
    `forge_repo_status` / `forge_sync` (they run plain `git pull/push`, host-agnostic) — now with the
    CPE-495 Sync dialog.
- **Frontend (`RepoBrowser.svelte`):** a *Generic Git (any URL)* provider option turns the field into a
  Git-URL input; **Browse is hidden** (no forge API — graceful degrade, with a hint that clone+sync
  still work). Clone routes through `cloneGeneric` → on a not-yet-admitted host it shows a **consent
  band naming the host** ("only this exact host, no wildcard") before any egress; Grant → `forge_admit_host`
  → `forge_clone_url`. A per-host token is remembered via the secrets broker keyed by the host.

Tradeoff / scope: browse for a generic remote is intentionally omitted (no universal forge API); the
threat-model write-up for push/write + generic-host egress is the sibling **CPE-499** (now unblocked).
Other named forges (GitLab/Bitbucket/Gitea manifests) remain out of this v2 wave.

## Work Log
2026-07-16 — Picked up (resequenced top of the CPE-488 children: ready + independent, unblocks CPE-499). Estimate: 3-4h.
2026-07-16 — Built the pure `repos::generic::parse_remote` (host + cred-stripped URL, transport allow-list) with 8 tests.
2026-07-16 — Backend: consent allow-list (fail-closed JSON in app_data), `forge_generic_remote`/`forge_admit_host`/`forge_forget_host`/`forge_admitted_hosts`/`forge_clone_url`, and pure `build_generic_clone` (non-admitted refusal + https token injection, ssh via agent). 5 lib tests. Registered all commands.
2026-07-16 — Frontend: Generic-Git provider in RepoBrowser — URL field, hidden Browse (graceful degrade), consent band naming the host before egress, per-host token via the secrets broker.
2026-07-16 — Verified: repos clippy + 8 generic tests green; app clippy (sidecar-platform) clean + 5 build_generic_clone tests; `npm run check` 0 errors; 497 frontend tests pass. All ACs met.

## Notes
Ties Q4 (Generic Git) + Q5 (self-hosted egress). Other forges (GitLab/Bitbucket/Gitea) are follow-up
manifests, out of this v2 wave.
