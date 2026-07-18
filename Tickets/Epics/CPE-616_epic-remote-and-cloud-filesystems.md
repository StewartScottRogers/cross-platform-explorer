---
id: CPE-616
title: "EPIC: Remote & cloud filesystems"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal

Let the explorer browse **beyond the local disk** — SFTP/SSH, network shares (SMB/UNC), WebDAV, and
optionally cloud object storage (S3-compatible) — as first-class **locations** in the sidebar, with the
same navigate/preview/transfer UX as local folders. Fulfils the "general cross-platform explorer"
promise for the increasingly common case where files live elsewhere.

## Why

Files aren't only on the local disk anymore. A single explorer that reaches a home server over SFTP, a
NAS over SMB, and a bucket over S3 — with the same tabs, search, and (via [[CPE-613]]) transfer manager
— is a strong, differentiating capability. It's an **additive** surface: remove the remote locations and
the local explorer is unchanged.

## Rough scope (areas, not child tickets)

- A filesystem **abstraction** so the frontend treats a location by a provider interface (list, stat,
  read, write, mkdir, delete) rather than assuming local paths — the enabling refactor.
- Connection management: add/edit/remove a remote (host, auth, path), stored securely; a "Connections"
  sidebar section.
- Providers, phased: SFTP first (broad + a good crate story), then SMB/UNC (partly native on Windows),
  then WebDAV, then S3-compatible.
- Async, cancellable listing/transfer with clear latency/error states (remote ops are slow + fail).
- Credential handling via the OS keychain/credential store — never plaintext.

## Open questions (resolve at activation)

- How deep does the local-path assumption run in the frontend/backend? (This refactor is the crux and
  the main risk — scope it carefully before promising providers.)
- Auth scope for v1: password + SSH key for SFTP; defer OAuth cloud providers (Drive/Dropbox) to later?
- Where does this sit vs the sidecar-platform architecture — a sidecar per provider, or in core?
- Offline/caching behaviour and how transfers to/from remotes integrate with the [[CPE-613]] queue.
- Security review: credential storage, host-key verification, and the CSP/network posture.

## Definition of Done

- At least SFTP works end-to-end: connect, browse, preview, and transfer to/from local via the queue.
- Remote locations appear in the sidebar and behave like local folders for navigation + search-in-folder.
- Credentials are stored in the OS secure store; host keys are verified.
- The local explorer is byte-for-byte unaffected when no remote is configured (fast/small/predictable).
- The filesystem-provider abstraction is documented and unit-tested with a fake provider.
