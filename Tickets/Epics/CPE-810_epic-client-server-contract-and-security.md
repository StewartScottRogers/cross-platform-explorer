---
id: CPE-810
title: "EPIC: Decoupled Rust Server + transport-neutral typed contract + pluggable security"
type: Task
status: In Progress
priority: High
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-20
closed:
---

> **Activated 2026-07-20.** Decomposed into children CPE-811–820 (below). Coordinate the "Server
> decoupled from Tauri" pillar (CPE-814/815) with CPE-676 (in Doing) — both touch `App.svelte`/`lib.rs`.

## Goal

Factor the app into **GUI ⟂ transport ⟂ Server**, where the Server is a transport-agnostic Rust
service reached through a **versioned, transport-neutral contract**, guarded by a **pluggable,
composable security layer**. One codebase then serves two topologies with the GUI and the command
logic unchanged between them:

```
Remote:  GUI ──(network)──► Client(Rust) ──(RPC)──► Server(Rust)   [other machine / OS / off-platform]
Local:   GUI ──(in-process IPC)────────────────────► Server(Rust)   [same box — today's Tauri backend]
```

The Server is the **same component** in both; only the transport between GUI and Server changes.
Typed IPC bindings (the earlier `tauri-specta` idea) are **absorbed into this epic** as its first
slice — they stop being the goal and become the mechanism that makes the contract a first-class
artifact instead of stringly-typed `invoke("name")` calls.

## Why

Today the 113 backend commands are welded to Tauri (`tauri::command`, `AppHandle`, `ipc::Channel`)
and matched from the frontend by string name with no compile-time contract (~118 hand-declared TS
interfaces duplicate the Rust structs — pure drift surface; `src/lib/types.ts`'s `DirEntry` is a
verbatim hand-copy of the Rust struct). Making the contract real and the Server transport-agnostic:

- turns "renamed/changed a command" into a **compile-time break** in the GUI, not a runtime surprise;
- lets the identical GUI drive a **local** in-process explorer **or** a **remote/cross-OS** server —
  one build, two products;
- generalizes the proven `sidecar/host` + `sidecar/contract` pattern (a local-process Client/Server
  behind a versioned contract) across a network.

**Hard tiebreaker preserved:** local mode keeps the Server *in-process* with zero network and a
null/passthrough security stack, so the plain explorer stays fast/small/predictable. Remote is
additive and pays its own way — it never taxes the local path.

## Rough scope (areas, not child tickets)

- **Contract as source of truth.** The 113 commands become a versioned service definition; `tauri-specta`
  generates the local TS bindings, but the schema must be network-consumable so a remote Client(Rust)
  can speak it too. Versioning follows the sidecar-contract precedent (negotiate mismatched versions).
- **Server decoupled from Tauri.** Split domain logic into a pure `Server` crate + a thin Tauri adapter,
  removing `AppHandle`/`ipc::Channel` coupling so the Server can run headless and remote.
- **Pluggable transport.** `src/lib/invoke.ts` — the single call chokepoint — becomes the swap point for
  *local IPC vs. remote RPC*; GUI code above it never changes. The 3 streaming `ipc::Channel` commands
  get a network-streaming equivalent that preserves the streaming-liveness convention.
- **Security layer (its own significant sub-area).** Modular, extendable, multi-solution-capable — see below.
- **Remote Client + reference Server.** The Client(Rust) proxy + a deployable Server, proving the loop
  end to end.
- **Guards.** Contract-drift CI (regenerate-and-diff), busy-cursor preservation through the new transport,
  a local-fast benchmark guard, and a security default-deny test.

## Security layer — required properties

Security is **not** baked into the transport or into any of the 113 commands. It sits at the contract
boundary as a distinct layer of **composable providers behind common traits**; the Server logic stays
security-agnostic and receives an *already-authorized* request + a principal/capability context.

Three planes, each a trait with a registry of interchangeable providers:

| Plane | Concern | Example providers (swappable) |
|-------|---------|-------------------------------|
| Transport security | encrypt/authenticate the channel | TLS, mTLS, Noise · *(local: none/trusted)* |
| Authentication | who is this client? | API token, OAuth/OIDC, passkey, mTLS identity · *(local: implicit principal)* |
| Authorization | may *this principal* do *this op on this path*? | path-scope allowlist, capability grants, role policy, consent prompt |

Cross-cutting **audit** formalizes the existing `audit_journal.rs` / `keyverify.rs` / `ConsentState`
/ egress-consent machinery rather than reinventing it.

The two properties explicitly required:
- **"Update and extend as needed"** → adding a provider = implement the trait + register; **no core
  changes**. Which providers are active, their order, and combine rules are **config-driven**.
- **"Run multiple security solutions at once"** → each plane is an **ordered interceptor chain** with a
  *combine policy* (`all-must-pass` / `any-passes` / `first-match`). E.g. accept **either** an API key
  **or** an OAuth token (AuthN = any-passes) **while** always enforcing path-scoping (AuthZ = all-must-pass).

Two non-negotiable invariants:
- **Default-deny at the boundary** — structurally impossible to leave one of the 113 commands
  unsecured (not a per-command checklist).
- **Local = null/passthrough** — trusted in-process principal, no transport crypto; security bills only
  in remote mode, protecting the local tiebreaker.

## Sequencing intent

The **first shippable increment is local typed bindings** (contract + Server-decoupling), useful on its
own and de-risking everything. Critical discipline: build it so the **seams** for transport, security,
and remote exist from day one — contract transport-neutral, Server Tauri-free, boundary already routing
through a locally no-op security stack. Otherwise the first slice hardens into Tauri-specific shape and
the later pillars need a rewrite.

## Open questions (resolve at activation)

- **Neutral contract format:** reuse serde-JSON over an RPC framing we own, vs. adopt gRPC/protobuf or
  Cap'n Proto? (Trade cross-language reach + tooling against dependency weight and the local-fast path.)
- **Single-user-per-instance vs. multi-client server?** ("my laptop driving my home server" vs. "many
  GUIs against one shared server") — changes auth/session/state substantially.
- **How deep is the Tauri coupling** in the 113 commands, and how much of `lib.rs` (~9k lines) must move
  to a pure `Server` crate? Scope this before promising the split.
- **Streaming over the wire:** WebSocket / SSE / gRPC-stream for the 3 `ipc::Channel` producers?
- **Where does the busy-cursor + Diagnostics timing live** once bindings are generated (they bypass
  `src/lib/invoke.ts` by default)? Point generated bindings at the `withBusy` wrapper; keep `rawInvoke`
  for streaming.
- **Feature-gated commands:** 140 items sit behind `feature = "sidecar-platform"`; codegen must run with
  that feature ON (one superset contract) or the frontend loses types for the AI-Console/Agent-Watch/Repos
  surface.
- **Security review** gating remote: transport crypto, credential storage (OS keychain), authZ path
  scoping, and whose filesystem/paths the remote actually exposes.

## Definition of Done (epic-level — refined at activation)

- A **versioned, transport-neutral contract** is the single source of truth; the frontend's duplicated TS
  interfaces are generated from it, and a CI guard fails on drift.
- The **Server** runs decoupled from Tauri; local mode drives it in-process, remote mode drives it via a
  Client(Rust) proxy over the network — the **same GUI**, unchanged.
- The **security layer** ships as pluggable AuthN/AuthZ/Transport traits with a config-driven, composable
  chain; at least two interchangeable AuthN providers and one AuthZ (path-scope) provider prove the model,
  and a default-deny test guards the boundary.
- **Local explorer is byte-for-byte unaffected** and no slower when no remote/security is configured
  (benchmark guard), and the busy-cursor + streaming-liveness conventions survive the transport swap.
- The architecture (contract, Server split, transport seam, security planes) is documented under
  `docs/design/`.

## Relationship to other epics / prior context

- **Distinct from [[CPE-616]]** (Remote & cloud filesystems): CPE-616 browses *other people's* servers
  (SFTP/SMB/WebDAV/S3) as locations; **this epic runs our own Server remotely** with the GUI as a thin
  client. They can compose later (a remote Server could host CPE-616 providers) but are separate goals.
- **Builds on** the sidecar platform program (`sidecar/host` + `sidecar/contract`) — the same
  versioned-contract-between-processes pattern, generalized across a network.
- **Absorbs** the earlier informal "add `tauri-specta` typed bindings" scoping (Tauri v2, 113 commands,
  ~41 serde types, 3 streaming channels, no existing codegen) as its first slice.

## Decisions (activated 2026-07-20)

- **Server model:** *both, single-user first* — design the contract envelope + security for multi-client
  (reserve a `principal`/`session` slot), but ship single-user in v1.
- **Wire format:** *own serde-JSON RPC envelope*, reusing the `sidecar/contract` `ContractVersion` +
  `negotiate()` + Hello/Welcome + `schema_version` pattern. No new heavy RPC dep; `tauri-specta` still
  generates the local TS. (Rejected gRPC/protobuf and Cap'n Proto for dependency weight / tooling.)
- **First slice:** *local typed bindings first* (CPE-812/813) — ship value + kill the ~118 duplicated TS
  interfaces + drift-CI, while building the seams for the rest.
- **Security v1:** *full stack now* — multiple providers up front: AuthN (token + mTLS + OAuth/OIDC),
  AuthZ (path-scope + capability), transport security (TLS/mTLS), all behind the composable chain.
- **Research findings:** Tauri coupling is deep — 45 commands take `AppHandle`, 29 take
  `Window`/`State`/`Manager`, 6 emit events — so the decoupling is staged behind a `ServerCtx` trait
  (CPE-814) before the crate extraction (CPE-815). Coordinate with CPE-676.

## Child tickets
1. **CPE-811** — Transport-neutral contract envelope (serde-JSON RPC; reuse sidecar version/negotiate;
   reserve principal/session). Pure crate, headless. *Foundation — buildable now.*
2. **CPE-812** — tauri-specta typed bindings: derive/annotate the 113 commands + ~41 types, generate
   `commands.ts`, wire busy-cursor, codegen with `sidecar-platform` ON. **First shippable slice — now.**
3. **CPE-813** — Adopt generated bindings at the 9 invoke sites, delete duplicated TS interfaces, add
   drift CI. *(prereq: 812)*
4. **CPE-814** — `ServerCtx` trait abstracting `AppHandle`/`Window`/`State`/emit off the 45+29 coupled
   commands. **Large refactor — coordinate with CPE-676.**
5. **CPE-815** — Extract a pure `server` crate (Tauri-free domain logic behind `ServerCtx` + envelope);
   Tauri becomes a thin adapter. **Large refactor.** *(prereq: 814, 811)*
6. **CPE-816** — Security core: 3-plane traits + composable interceptor chain (combine policies) +
   default-deny + null/passthrough local + audit hook. *(prereq: 811)*
7. **CPE-817** — AuthN providers: API token + mTLS identity + OAuth/OIDC; prove `any-passes`. *(prereq: 816)*
8. **CPE-818** — AuthZ (path-scope + capability, `all-must-pass`) + transport security (TLS/mTLS). *(prereq: 816)*
9. **CPE-819** — Frontend pluggable transport seam (local IPC vs remote RPC) + streaming equivalent for
   the 3 Channel producers. *(prereq: 811, 815)*
10. **CPE-820** — Client(Rust) proxy + reference headless Server + end-to-end remote loop with security
    enforcing; local-fast benchmark + conformance guards. *(prereq: 815, 816, 819)*

## Work Log
- **2026-07-20** — Activated. Researched Tauri coupling depth (45 `AppHandle` / 29 `Window`/`State` / 6
  emit; 113 commands, ~41 serde types, 3 `ipc::Channel`, 140 items behind `sidecar-platform`) and the
  reusable `sidecar/contract` versioning. Resolved 4 decisions (above) with the user; decomposed into
  CPE-811–820. Suggested start: CPE-811 (foundation) or CPE-812 (first user-visible slice).
