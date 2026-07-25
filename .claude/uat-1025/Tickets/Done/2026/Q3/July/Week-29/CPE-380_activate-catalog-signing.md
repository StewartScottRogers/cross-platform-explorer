---
id: CPE-380
title: "Activate catalog signing: keygen helper + embed trusted pubkey"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [ready]
estimate: 30m
created: 2026-07-14
closed: 2026-07-14
---

## Summary

Turn the (dormant) catalog auto-update on: a `catalog-sign keygen` helper that generates the signing
keypair (private seed → gitignored file, public key → stdout), and embed the public key in
`CATALOG_TRUSTED_KEYS`. The operator then sets the private seed as the `CPE_CATALOG_SIGNING_KEY` CI
secret (I can't add repo secrets).

## Acceptance Criteria

- [x] `catalog-sign keygen <file>` writes the seed hex to a `*.key` (gitignored) file and prints the
      public key — the private seed never touches the chat transcript.
- [x] `CATALOG_TRUSTED_KEYS` (src-tauri) holds the generated public key; feature build compiles.
- [x] Operator instructions for `gh secret set CPE_CATALOG_SIGNING_KEY` + deleting the local key.

## Work Log
2026-07-14 — Activating per user request.
