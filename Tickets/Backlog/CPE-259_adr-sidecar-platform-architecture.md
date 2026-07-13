---
id: CPE-259
title: "ADR: Sidecar platform architecture"
type: Task
status: Open
priority: High
component: Docs
estimate: 3-4h
created: 2026-07-13
---

## Summary

The architecture decision record that governs the whole sidecar program.
Mega-Features (starting with the AI Console) are built as **standalone sidecar
processes** hosted by the explorer through **one small, versioned contract**. This
is the constitution the platform epic ([[CPE-260]]) and every tenant epic
([[CPE-261]] first) must obey. Nothing is built until it is agreed, because a weak
boundary here is the one mistake that cannot be refactored away later.

## The five principles (from the user)

1. **Standalone-capable** — a sidecar can run as its own app; the explorer is one host.
2. **Built for hugeness** — grows forever to track the coding-agent market; growth is additive.
3. **Zero ricochet** — a change lands only where it belongs; no rippling into the explorer or other sidecars.
4. **Plugin-but-more-flexible** — a decoupled subsystem behind a stable contract, not a rigid API.
5. **No entanglement / ultra-clear boundaries** — one thin, explicit seam; nothing else shared.

## Invariants (non-negotiable)

- **One-way dependency.** Sidecars depend only on the `sidecar-contract` crate;
  they never import explorer internals, and the explorer never imports a sidecar's
  internals. Enforced by a CI lint ([[CPE-272]]).
- **Process isolation.** Each sidecar is its own OS process = its own crash domain,
  memory space, storage scope ([[CPE-269]]), and secrets scope ([[CPE-268]]).
- **No ambient authority.** A sidecar gets nothing by default; it *requests*
  capabilities, the user/host *grants* scoped ones ([[CPE-266]], [[CPE-296]]).
- **No cross-sidecar visibility.** Sidecars cannot discover or talk to each other
  except through explicit host-brokered channels ([[CPE-275]]).
- **The contract is the only throttle.** It stays small, additive, and semver'd;
  anything that could ripple must pass through it and its version negotiation
  ([[CPE-263]]).

## Decisions

- **Isolation model:** out-of-process **sidecar** (chosen over in-process crate or
  separate-repo-now). Strongest isolation + genuine standalone story.
- **UI model:** each sidecar **serves its own UI**; the host embeds it in a
  sandboxed frame ([[CPE-271]]). Maximises isolation and standalone reuse.
- **Contract transport:** decided by the Phase-0 spike ([[CPE-294]]) — candidates:
  length-prefixed JSON/CBOR over stdio pipes, or a local socket. Must support
  streaming + backpressure for PTY/install output.
- **Trust model:** bundled manifests are first-party; **user/third-party manifests
  are untrusted executable content** and require explicit provenance + consent
  before any command runs ([[CPE-295]]). This is the load-bearing security
  decision — an agent manifest defines install/run commands that execute arbitrary
  code with the user's credentials.
- **Versioning:** the contract, sidecar manifests, agent manifests, stored state,
  and credential-profile formats **all** carry a schema version with a documented
  migration path ([[CPE-300]]).

## The enforcement rule — the delete-test

> Remove every sidecar → the explorer still builds, ships, and runs
> fast/small/predictable. Remove one sidecar → nothing else notices.

This single test is the boundary's guarantee, wired into CI ([[CPE-272]]). It is the
build/coupling analogue of PURPOSE.md's "with a mode off, the explorer stays
fast/small/predictable."

## Quality bars & budgets (must be met to ship)

- **Startup:** zero measurable startup cost with all sidecars disabled.
- **Footprint:** documented per-sidecar memory ceiling; supervisor enforces it
  ([[CPE-297]]).
- **Security:** passes the end-to-end threat model / review ([[CPE-304]]); secrets
  never on disk in plaintext, never logged, never in a webview ([[CPE-268]]).
- **Observability:** every sidecar failure is diagnosable from host-side logs
  ([[CPE-298]]) and surfaced with an actionable error ([[CPE-299]]).
- **Testability:** a sidecar proves compliance via the conformance kit ([[CPE-301]]);
  the platform has an E2E harness ([[CPE-302]]).

## Non-goals (explicit)

- Not a general plugin marketplace/sandboxed-WASM runtime (sidecars are native
  processes we trust per the trust model, not arbitrary downloaded code run blindly).
- Not sidecar-to-sidecar orchestration in v1 (all coordination via the host).
- The explorer does not gain awareness of any specific sidecar's domain.

## Relationship to Agent Watch

Agent Watch (AGENT-WATCH.md) **observes** filesystem activity and explicitly has
"no control surface." The AI Console **drives** an agent. They are complementary,
distinct surfaces and should integrate ([[CPE-305]]): launching an agent in the
console can light up Agent Watch on that repo.

## Deliverable

`docs/adr/0001-sidecar-platform.md` capturing all of the above, plus a glossary
(host, sidecar, tenant, contract, capability, manifest) and a diagram of the seam.
Linked from PURPOSE.md and CLAUDE.md.

## Acceptance Criteria

- [ ] ADR published at the path above and linked from PURPOSE.md/CLAUDE.md.
- [ ] Records principles, invariants, decisions, delete-test, quality bars,
      non-goals, trust model, versioning policy, and the Agent Watch relationship.
- [ ] Every cross-cutting concern has a home ticket referenced here.
- [ ] Reviewed/agreed before [[CPE-260]] P1 begins.

## Notes — Dependencies / Schedule
**Depends on:** none (but informed by the spike [[CPE-294]]). **Phase:** Foundation.
Blocks all of [[CPE-260]] and [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Expanded to a full architecture charter: added invariants, trust
model, quality bars/budgets, versioning policy, non-goals, and links to the new
cross-cutting tickets ([[CPE-294]]–[[CPE-314]]).
