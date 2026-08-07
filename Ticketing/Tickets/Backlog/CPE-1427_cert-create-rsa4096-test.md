---
id: CPE-1427
title: "Test: add RSA-4096 round-trip coverage to cert_create"
type: Task
status: Backlog
priority: Low
component: Backend
tags: [ready]
epic: CPE-1417
created: 2026-08-07
---
## Problem (CPE-1420 / PR #694 reviewer note)
`cert_create` supports RSA-4096 (`generate_rsa_key_pair(4096)`) but only RSA-2048 has an explicit round-trip
test. Same parameterized path, low risk, but add an RSA-4096 create→cert_decode round-trip + key-pairs-with-cert
test for full key-size coverage. `crates/server/src/cert_create.rs` tests.
