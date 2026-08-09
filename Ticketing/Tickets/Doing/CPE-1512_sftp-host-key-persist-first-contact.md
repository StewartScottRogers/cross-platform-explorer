---
id: CPE-1512
title: "Persist first-contact SFTP host keys to an app-managed known_hosts (complete TOFU)"
type: Bug
status: Backlog
priority: Medium
component: Backend
tags: [ready, security]
epic: CPE-616
parent: CPE-1499
created: 2026-08-08
---
## Vector (found by CPE-1511's adversarial review)
CPE-1511 wired SFTP/WebDAV browsing with `HostKeyPolicy::Tofu`. It reads the user's real `~/.ssh/known_hosts`
(so a host the user already pinned via OpenSSH gets genuine CHANGED-key MITM protection), and a CHANGED/REVOKED
key refuses loudly. BUT there is **no `save_known_hosts`/persist path** (`known_hosts.rs` only loads/parses/
verifies), and `presented_host_key` is never read by non-test code. So a **first-contact** host (not already in
`~/.ssh/known_hosts`) is accepted under Tofu and **never written back** → effectively **trust-on-every-use** for
hosts only the app ever touches: a later MITM presenting a different key is again just `Unknown` → silently
accepted; the CHANGED path can never fire for app-only hosts.

## Not a regression, but a real completeness gap
`origin/main` had no remote at all, and CHANGED protection IS real for OpenSSH-pinned hosts — so this is not a
regression and CPE-1511's own AC ("changed key refuses loudly") holds for pinned hosts. But TOFU is incomplete
without persistence.

## Fix
After a successful **first-contact (`Unknown`)** SFTP connect, persist the presented host key to an
**app-managed** known_hosts store (NOT the user's `~/.ssh/known_hosts` — never silently mutate that), so
subsequent connects to that host resolve to `Trusted` (or `Changed` on a key swap → loud refuse). Add
`save_known_hosts`/an append to `known_hosts.rs`, wire the SFTP connect path to record `presented_host_key` on
first contact, and merge the app store with `~/.ssh/known_hosts` at verify time (user's pins win). Headless-
testable: first connect records the key; a second connect with the SAME key → Trusted (no reprompt); a second
connect with a DIFFERENT key → Changed → refused. Surface a first-contact "trust this host?" affordance in the
Network UI later (CPE-1498) — for now recording-on-first-use is the baseline (document the UX choice).

## Notes
Epic CPE-616 / Network program. Security-completeness. Filed from CPE-1511 review.
