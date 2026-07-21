# Per-tenant sidecar security checklist (CPE-304)

A lightweight review every **new tenant sidecar** must pass before it ships, derived from
the full [`threat-model.md`](threat-model.md). Copy this into the tenant's PR and tick each
box. Anything unchecked is a blocker.

## Boundary & isolation
- [ ] Runs as its **own OS process** (own crash/kill/memory domain); no in-host threads.
- [ ] Depends **only** on `sidecar-contract` — no explorer internals, no other sidecar
      (one-way rule; CI grep guard passes).
- [ ] Handshake uses the per-launch token (`CPE_SIDECAR_TOKEN`); rejects a bad/absent token.

## Capabilities (least privilege)
- [ ] Requests the **minimum** capabilities it needs; each is justified in the manifest.
- [ ] Behaves passively when the host grants nothing (respects `capabilities_granted`).
- [ ] No capability effect is attempted outside the broker.

## Secrets
- [ ] Secrets only via the `secrets.*` broker; **never** written to the sidecar's own files.
- [ ] No secret value appears in any `Event`/`Status`, log line, or the served UI.
- [ ] Secret names are namespaced to this sidecar; no attempt to read another's keys.

## Manifests & spawned processes
- [ ] Bundled manifests are first-party; any user/third-party manifest is gated behind
      provenance + consent (no command runs before consent — CPE-296).
- [ ] Dangerous flags on any spawned CLI are surfaced (`scope::dangerous_flags` or equiv).
- [ ] Child processes (agent/MCP) are spawned/killed/reaped cleanly — no orphans on exit.

## Embedded UI
- [ ] Serves its UI on **loopback only**; announces via `ui:<127.0.0.1 url>`.
- [ ] Assumes the cross-origin sandbox (`allow-scripts allow-forms allow-same-origin`,
      CPE-334); `allow-same-origin` grants same-origin only to the frame's **own loopback
      origin** (for clipboard/WebGL), never to the host — never expects Tauri/parent access.
- [ ] Receives no raw secrets in the webview; triggers secret use by name.

## Observability
- [ ] Failures are diagnosable from host-side logs; errors are actionable (CPE-298/299).
- [ ] Passes the conformance kit as a real process.

## Sign-off
- [ ] Reviewer + date recorded; any gap filed as a blocker ticket and linked here.

### Tenant sign-off log
- **Agent Deck (CPE-261)** — reviewed 2026-07-14 (CPE-304 final pass). All boxes pass on
  **Windows** (bundled first-party manifests only; keychain round-trip verified). Cross-OS
  secret-store persistence pending runtime QA on real macOS/Linux hardware → blocker
  **CPE-322**. Windows-first sign-off: **granted**. Cross-OS sign-off: **deferred** to
  CPE-322.
