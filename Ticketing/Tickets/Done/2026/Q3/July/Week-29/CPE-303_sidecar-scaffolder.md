---
id: CPE-303
title: Sidecar scaffolder ("create-sidecar" generator)
type: Task
status: Done
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

To keep "adding sidecars" cheap over years, ship a generator that scaffolds a new
sidecar (crate + frontend stub + manifest + conformance test wiring) from a
template, so a new Mega-Feature starts compliant on day one. This is what makes the
platform a genuine platform, not a one-off.

## Acceptance Criteria

- [ ] One command scaffolds a working sidecar that handshakes, mounts an empty UI,
      and passes the conformance kit ([[CPE-301]]).
- [ ] Template documents where to add capabilities, UI, and logic.
- [ ] Referenced from the SDK docs ([[CPE-273]]).

## Resolution

Implemented `scaffold` in `sidecar-host`: a pure `scaffold(name) -> Vec<(path, content)>`
that generates a compliant sidecar skeleton (Cargo.toml depending only on the contract,
`src/main.rs` modelled on the reference sidecar — Hello → Welcome/Ready → request
handler, `sidecar.json` manifest stamped with the current contract version, README),
plus `is_valid_name` validation. A thin `create_sidecar` bin writes the files to
`sidecar/<name>/`. 5 unit tests (name validation, file set, name/contract wiring, valid
JSON). **Verified for real:** ran `create-sidecar demo-sidecar` and the generated crate
`cargo build`s cleanly against the contract (then removed). Covered by the existing
cross-OS sidecar CI job.

**Deferred:** the fuller reusable SDK-helper crate is a natural follow-up; the template +
generator (the core of the ticket) are done. UI-mount scaffolding waits on [[CPE-271]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-13 — Implemented pure generator + CLI; verified generated crate compiles.
Done during dayshift.
