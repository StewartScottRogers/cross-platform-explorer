---
id: CPE-386
title: "Consent: leaving a sensitive capability off must not silently DENY it (dead-ends Keys)"
type: Bug
status: Done
priority: High
component: Multiple
tags: [ready]
created: 2026-07-14
closed: 2026-07-14
---

## Summary

Saving a provider key fails with "Secrets is not granted to 'ai-console'". Root cause: the consent
sheet defaults sensitive caps (secrets/network) to OFF, but `decide()` marks **every** requested cap
as "decided", so an un-checked Secrets is recorded as a hard **deny** (`consent.json`:
`denied: ["secrets"]`). Denied ≠ undecided → the console never re-prompts → the Keys feature is a
permanent dead-end with no obvious recovery.

## Fix

- `ConsentSheet.decide()`: a sensitive cap left at its default-off (not previously granted) stays
  **undecided** — not denied — so opening the console re-prompts and the user can grant it. Only an
  actively-granted cap, an actively-unchecked previously-granted cap, or a non-sensitive cap counts
  as "decided".
- Keys panel (launcher): a "Secrets not granted" error becomes actionable ("reopen the console and
  allow Secrets") instead of a raw broker message.

## Acceptance Criteria

- [x] Leaving Secrets off no longer writes `denied: ["secrets"]`; it's left undecided.
- [x] The console re-prompts for the undecided Secrets on next open.
- [x] The Keys panel error explains what to do.
- [x] `npm run check` clean; ai-console builds; clippy clean.

## Work Log
2026-07-14 — Root-caused from the installed consent.json (secrets denied). Unblocked the user by
granting secrets in their store; now fixing so it can't recur.
