---
id: CPE-897
title: SFTP host-key verification core (known_hosts parse + TOFU / changed-key detection)
type: feature
component: Backend
priority: medium
tags: ready
epic: CPE-616
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
The headless security core of the SFTP provider (CPE-682 AC #2: "Host key verified before any op").
Decoupled from any network/ssh crate so it's pure + unit-testable: given the lines of a `known_hosts`
file and the host key a server presents, decide whether to trust it — the OpenSSH **trust-on-first-use**
(TOFU) + "a *changed* key is refused loudly" model that actually defends an SFTP session against a MITM.

New `cpe-server::known_hosts`:
- `parse_known_hosts(contents) -> Vec<KnownHost>` — parses `patterns keytype base64key` lines, skipping
  blanks, `#` comments, `@`-marker lines, and malformed (<3-field) lines; splits comma pattern lists.
- `host_token(host, port)` — the bare `host` on port 22, else the bracketed `[host]:port` OpenSSH writes.
- `verify_host_key(known, host, port, key_type, key_b64) -> HostKeyVerdict` — `Trusted` (stored key for
  this host+type matches), `Changed` (host+type known but key differs → possible MITM, refuse loudly), or
  `Unknown` (no entry → first contact, prompt to trust). Safe default is always `Unknown`, never a false
  `Trusted`.

## Decisions
- **Match on host-token AND key-type together.** A stored key of a *different* type never triggers
  `Changed` (OpenSSH would just add the new type); only a same-host, same-type, different-key is a changed
  key.
- **Scope: plain host patterns + `[host]:port` + comma lists.** Hashed hostnames (`|1|salt|hash`, needs
  hmac-sha1), wildcard/negated patterns, and `@revoked`/`@cert-authority` markers are out of this slice —
  a non-matching/marker line simply yields `Unknown` (safe: prompt), never a false trust. Hashed support
  is a documented follow-up.

## Acceptance Criteria
- [x] Parse a `known_hosts` file (multi-pattern lines, port tokens, comments/blank/marker/malformed skipped).
- [x] `Trusted` / `Unknown` / `Changed` verdicts, with `Changed` reserved for same host+type, different key.
- [x] Default-port vs `[host]:port` lookups don't cross-match.
- [x] `cargo test` (5 new) + `cargo clippy --all-targets -D warnings` clean.

## Work Log
- 2026-07-22 (nightshift) — Carved the headless security half of CPE-682. The remaining SFTP provider
  (connect/list/stat/read via an ssh crate, host-key check wired to this at connect time) still needs
  network + attended testing against a real server — kept on CPE-682.
