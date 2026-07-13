# Sidecar platform — threat model & security review (CPE-304)

**Scope:** the whole sidecar boundary — IPC channel, capability broker, secrets broker,
manifest trust, embedded UI/CSP, and spawned agent/MCP processes. **Method:** STRIDE per
surface. **Status legend:** ✅ implemented & tested · 🟡 partial/gated · ⛔ not yet built
(gap filed). This is a living document; re-run per new tenant sidecar using
[`sidecar-review-checklist.md`](sidecar-review-checklist.md).

> **Sign-off status: NOT production-signed-off.** The design mitigates every identified
> threat, and the core is implemented and tested, but two mitigations are incomplete:
> capability **consent UX** (CPE-296, ⛔ backend-ready/UI-pending) and **non-Windows
> keychains** (CPE-322, 🟡). Production sign-off is gated on those. See §8.

## Assets & trust boundaries

- **Assets:** provider API keys / credentials; the user's filesystem & shell; explorer
  process integrity; the host↔sidecar channel; agent manifests (executable intent).
- **Boundaries:** (a) explorer host ⇄ sidecar process (OS process boundary + IPC);
  (b) host ⇄ embedded sidecar UI (iframe origin boundary); (c) sidecar ⇄ spawned agent
  CLI / MCP server (PTY/child-process boundary); (d) first-party bundled manifests ⇄
  user/third-party manifests (trust boundary).
- **Adversaries:** a malicious or compromised sidecar; a malicious agent manifest; a
  malicious page loaded in an embedded UI; a local process trying to impersonate a
  sidecar; a curious user reading logs/disk for secrets.

## 1. IPC channel (host ⇄ sidecar over stdio)

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **S**poofing | A local process impersonates the sidecar / connects to the host. | Per-launch random token: host generates it, passes via `CPE_SIDECAR_TOKEN` (`AUTH_TOKEN_ENV`) to the child only, and rejects any `Hello` whose `auth_token` doesn't match (CPE-275). Transport is the child's own stdio pipe — not a shared socket — so there is no port to connect to. | ✅ |
| **T**ampering | Frames altered in flight. | In-process OS pipe between parent and its own child; no network hop. Schema-versioned `Envelope` with strict decode; undecodable lines are skipped, not trusted. | ✅ |
| **R**epudiation | Can't tell which sidecar did what. | Structured host-side logs per sidecar id (`observability`), correlation ids on request/response. | ✅ |
| **I**nfo disclosure | Secrets leak through the channel/logs. | `Redactor` scrubs secret values from logs (`redact_str`/`redact_json`); secrets flow only in `secrets.*` responses, never in events/status. | ✅ |
| **D**oS | A chatty sidecar floods the host. | Bounded `sync_channel` (`IPC_CHANNEL_CAPACITY`) gives backpressure; resource budgets sample memory (CPE-297). | ✅ |
| **E**oP | Sidecar drives the host beyond its grant. | The channel carries only contract messages; capability effects go through the broker (§2), never raw host calls. | ✅ |

## 2. Capability broker

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **E**oP | Sidecar uses a capability it wasn't granted. | Granted set = **`requested ∩ consented ∩ policy`** (`broker::decide_grants`) — least privilege; a capability absent from `capabilities_granted` is refused at the provider. No ambient authority. | ✅ |
| **Spoofing/Tampering** | Sidecar claims a broader grant than consented. | Grants are computed host-side from the consent set, never taken from the sidecar's request alone. | ✅ |
| **Consent integrity** | Capabilities granted without the user actually agreeing. | Design: `consented` comes from an explicit user prompt (CPE-296). **Today** bundled first-party sidecars are auto-consented to their declared set; the interactive consent prompt UI is **not yet built**. | ⛔ CPE-296 |

## 3. Secrets broker

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **I**nfo disclosure (at rest) | Plaintext secrets on disk. | Backed by the OS keychain via `keyring` (`providers::secrets`); no secret file. Verified round-trip on Windows Credential Manager. | 🟡 CPE-322 (macOS/Linux fall back to in-memory — no persistence, not plaintext-on-disk) |
| **I**nfo disclosure (in transit/logs) | Secret in a log line or the UI. | `Redactor` scrubs values from all structured logs; secrets are never sent in events/status and never cross into the webview (the UI never receives raw keys — it triggers `secrets.*` by name). | ✅ |
| **E**oP / isolation | One sidecar reads another's secrets. | Per-sidecar namespace on the secrets provider — a sidecar can only address its own keys. | ✅ |
| **Tampering** | Sidecar overwrites host/other credentials. | Namespaced writes; the provider keys are scoped by sidecar id. | ✅ |

## 4. Manifest trust (sidecar & agent manifests)

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **Tampering / Spoofing** | A manifest is altered or a malicious one is dropped in. | First-party manifests are **bundled** (ship inside the signed app, never downloaded — CPE-276). Integrity via `content_hash` (sha256) and ed25519 `verify_signature` against a `TrustStore` (CPE-295). | ✅ |
| **E**oP (code execution) | A user/third-party manifest runs arbitrary commands. | User/third-party manifests are treated as **untrusted executable content**: unsigned/unknown-provenance manifests require explicit consent before any command runs (CPE-295/296). Bundled ≠ user dir; user dir overrides only by explicit id and is flagged. | 🟡 (verification ✅; the consent-gate UI is CPE-296 ⛔) |
| **R**epudiation | No record of which manifest was trusted. | `TrustDecision` records provenance; host logs the trust outcome. | ✅ |

## 5. Embedded UI / CSP

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **E**oP (UI escape) | The sidecar UI reaches explorer internals / Tauri APIs. | Embedded in `<iframe sandbox="allow-scripts allow-forms">` **without** `allow-same-origin` ⇒ opaque origin: no access to the parent DOM, cookies, storage, or `window.__TAURI__`. The iframe cannot invoke Tauri commands. | ✅ |
| **Spoofing** | UI URL points somewhere malicious. | `parseUiAnnouncement` accepts **loopback-only** URLs; the sidecar serves its own UI on `127.0.0.1`. | ✅ |
| **I**nfo disclosure | UI exfiltrates via network. | Opaque-origin sandbox + loopback UI; no secrets are delivered to the webview (§3). | ✅ |
| **T**ampering | Parent page tampered by the frame. | Sandbox blocks top-navigation and same-origin access; host↔UI messaging is not wired to privileged APIs. | ✅ |

## 6. Spawned agent CLI & MCP processes

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **E**oP | An agent CLI is arbitrary code with the user's privileges. | This is inherent to "run a coding agent" — the user explicitly launches it. Scoped by design: launched only from a consented manifest; `scope::dangerous_flags` surfaces risky flags; the agent runs as a child of the sidecar (its own crash/kill domain), not the host. | 🟡 (surfaced; a hard sandbox of the agent is out of scope — see §7) |
| **I**nfo disclosure | Credentials injected into the agent's env leak. | Keys are resolved from the keychain and injected into the child env at spawn (`vault::resolve_env`), never written to disk or logged (`Redactor`). | ✅ |
| **D**oS / orphans | Agent/MCP processes leak or wedge. | Supervisor spawn/kill/reap with restart policy; PTY drain avoids the ConPTY hang; MCP lifecycle is managed (`mcp`). | ✅ |
| **Spoofing** | A rogue MCP server impersonates a trusted one. | MCP servers are declared per-agent manifest (trusted like the manifest, §4); no auto-discovery of arbitrary servers. | ✅ |

## 7. Explicit non-goals / accepted risks

- **The agent itself is trusted-by-user.** The platform's job is to isolate the *sidecar
  and secrets*, surface risk, and require consent — not to sandbox a coding agent the user
  deliberately runs on their own repo. A hard OS-sandbox of the agent (seccomp/AppContainer)
  is a future hardening, not a v1 promise.
- **Not sidecar-to-sidecar orchestration** — no cross-sidecar channel exists to attack
  (ADR 0001).

## 8. Verification of the required invariants

| Invariant (from CPE-304) | Result |
|--------------------------|--------|
| No plaintext secrets at rest | ✅ Windows (keychain). 🟡 macOS/Linux use in-memory (no disk, no persistence) until CPE-322. |
| No secret in logs / UI | ✅ `Redactor` + secrets never delivered to the webview. |
| No cross-sidecar reach | ✅ Per-process isolation; namespaced storage/secrets; no cross-sidecar channel. |
| No unconsented code execution | 🟡 Trust/verification ✅; the interactive **consent gate UI is CPE-296 (⛔)**. Bundled first-party is auto-consented; user/third-party manifests must not run until CPE-296 lands. |
| No UI escape to explorer | ✅ Opaque-origin sandboxed iframe, loopback-only. |

## 9. Gaps → tickets (sign-off blockers)

- **CPE-296** — capability & manifest **consent UX**. Until this ships, the "no unconsented
  code execution" invariant holds only because untrusted manifests are refused/auto-scope
  is limited to bundled first-party. **Blocks production sign-off.**
- **CPE-322** — macOS/Linux **OS-keychain** backends. Until this ships, secrets don't
  persist in a native store off-Windows, so the sidecar release stays Windows-only.
  **Blocks cross-OS production sign-off.**

When CPE-296 and CPE-322 are Done, re-run this review and record final sign-off in
`docs/adr/0001-sidecar-platform.md`.
