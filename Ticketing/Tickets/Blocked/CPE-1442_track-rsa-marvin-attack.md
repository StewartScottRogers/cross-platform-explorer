---
id: CPE-1442
title: "Track: rsa RUSTSEC-2023-0071 (Marvin Attack) — await rsa 0.10 stable"
type: Bug
status: Blocked
priority: Low
component: Backend
tags: [resource-blocked, upstream]
epic: CPE-1417
created: 2026-08-07
---
## Advisory (found by the shift-1 dependency audit)
`rsa 0.9.10` (direct dep from the crypto epic CPE-1420, `crates/server/src/cert_create.rs`) carries
**RUSTSEC-2023-0071** — the Marvin Attack: a timing side-channel that can recover an RSA key via a remote
timing oracle on the decrypt/verify path.

## Why Blocked (external), and why LOW real risk here
- **No fixed release exists** — the fix landed only in `rsa 0.10`, still an RC as of 2026-08-07. Blocked on
  upstream shipping 0.10 stable.
- **Exploitable pattern is not reachable here:** this codebase calls `rsa` only for local
  `RsaPrivateKey::new(...)` keygen (cert creation). Certificate signing is delegated to `rcgen`'s `ring`
  backend, and nothing exposes an `rsa` decrypt/verify timing oracle to a network attacker. The Marvin Attack
  needs a remote timing oracle, which doesn't appear in our usage.

## Unblocks when
`rsa 0.10` ships stable → bump `crates/server` to it, re-run `cargo audit`, confirm RUSTSEC-2023-0071 clears.
Until then, tracked-not-fixable. Revisit on any future `cargo audit` sweep.

## Notes
Dependency Steward finding, shift-1 audit 2026-08-07. Keep the pin as-is; do not downgrade functionality for a
non-reachable advisory. Re-check upstream rsa releases periodically.
