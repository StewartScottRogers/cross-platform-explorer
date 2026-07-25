# Sidecar platform — threat model & security review (CPE-304)

**Scope:** the whole sidecar boundary — IPC channel, capability broker, secrets broker,
manifest trust, embedded UI/CSP, spawned agent/MCP processes, and host-mediated network
egress. **Method:** STRIDE per surface. **Status legend:** ✅ implemented & tested · 🟡 partial/gated · ⛔ not yet built
(gap filed). This is a living document; re-run per new tenant sidecar using
[`sidecar-review-checklist.md`](sidecar-review-checklist.md).

> **Sign-off status: WINDOWS-FIRST SIGNED-OFF (2026-07-14, CPE-304 final pass); cross-OS deferred.**
> The final review pass verified every mitigation below against the current code (broker
> `decide_grants`, `Redactor`, per-sidecar keychain namespace, the `verify_key` 3-endpoint
> allow-list, and the loopback iframe sandbox). Capability **consent UX (CPE-296) is DONE**. On
> **Windows** every mitigation is implemented, tested, and the secret store is a real OS keychain
> (round-trip verified) — the shipping (Windows-first, bundled-first-party-only) scope is
> **signed off**. The **cross-OS** sign-off is **deferred**: macOS/Linux keychain backends are
> coded and CI-compile-verified but await a runtime store/get/delete round-trip on real hardware
> (**CPE-322**, Blocked — needs a mac/Linux desktop). See §9/§10.

## Assets & trust boundaries

- **Assets:** provider API keys / credentials; the user's filesystem & shell; explorer
  process integrity; the host↔sidecar channel; agent manifests (executable intent).
- **Boundaries:** (a) explorer host ⇄ sidecar process (OS process boundary + IPC);
  (b) host ⇄ embedded sidecar UI (iframe origin boundary); (c) sidecar ⇄ spawned agent
  CLI / MCP server (PTY/child-process boundary); (d) first-party bundled manifests ⇄
  user/third-party manifests (trust boundary); (e) host ⇄ external provider API on the
  sidecar's behalf (allow-listed network egress, §7).
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
| **Consent integrity** | Capabilities granted without the user actually agreeing. | `consented` comes from an explicit user prompt — the **consent sheet** (`ConsentSheet.svelte`, CPE-296): per-capability grant/deny with plain-language descriptions + a sensitive-risk badge (secrets/network default off), shown on first run and re-prompting for any newly-requested capability after an update. Grants persist and are revocable in the manager (CPE-274). | ✅ |

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
| **E**oP (code execution) | A user/third-party manifest runs arbitrary commands. | User/third-party manifests are treated as **untrusted executable content**: unsigned/unknown-provenance manifests require explicit consent before any command runs (CPE-295/296). Bundled ≠ user dir; user dir overrides only by explicit id and is flagged. The capability consent UI (CPE-296) is done; the shipped Agent Deck runs only bundled first-party (signed) manifests — no untrusted-manifest loading is exposed. | ✅ |
| **R**epudiation | No record of which manifest was trusted. | `TrustDecision` records provenance; host logs the trust outcome. | ✅ |
| **Tampering / Rollback** | A runtime **catalog update** ships a tampered or stale-but-signed agent manifest (RCE via a swapped install/run recipe, or replaying an old bad recipe). | Host-authoritative catalog index (`host::catalog`, CPE-308/371): the index is ed25519-verified against a trusted key, each entry is **content-bound** by sha256, and entries are **anti-rollback** (strictly-monotonic `version`). The sidecar then re-verifies each manifest's signature on load (`ai_console::catalog`), defence-in-depth. The remote **fetch** that delivers a catalog is deferred (CPE-308 part 2) and will be host-mediated + allow-listed, extending §7. | 🟡 (index verify + anti-rollback ✅; host-mediated fetch pending — part 2) |

## 5. Embedded UI / CSP

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **E**oP (UI escape) | The sidecar UI reaches explorer internals / Tauri APIs. | Embedded in `<iframe sandbox="allow-scripts allow-forms allow-same-origin">`. The frame's origin is the sidecar's **loopback URL**, which differs from the explorer's app origin, so the same-origin policy still blocks access to the parent DOM, storage, and `window.__TAURI__` — it cannot invoke Tauri commands. `allow-same-origin` (CPE-334) lets the frame use clipboard + WebGL **for its own loopback origin only**; it does not make it same-origin with the host. No `allow-top-navigation`/`allow-popups`. | ✅ |
| **Spoofing** | UI URL points somewhere malicious. | `parseUiAnnouncement` accepts **loopback-only** URLs; the sidecar serves its own UI on `127.0.0.1`. | ✅ |
| **I**nfo disclosure | UI exfiltrates via network. | Opaque-origin sandbox + loopback UI; no secrets are delivered to the webview (§3). | ✅ |
| **T**ampering | Parent page tampered by the frame. | Sandbox blocks top-navigation and same-origin access; host↔UI messaging is not wired to privileged APIs. | ✅ |

## 6. Spawned agent CLI & MCP processes

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **E**oP | An agent CLI is arbitrary code with the user's privileges. | This is inherent to "run a coding agent" — the user explicitly launches it. Scoped by design: launched only from a consented manifest; `scope::dangerous_flags` surfaces risky flags; the agent runs as a child of the sidecar (its own crash/kill domain), not the host. | 🟡 (surfaced; a hard sandbox of the agent is out of scope — see §8) |
| **I**nfo disclosure | Credentials injected into the agent's env leak. | Keys are resolved from the keychain and injected into the child env at spawn (`vault::resolve_env`), never written to disk or logged (`Redactor`). | ✅ |
| **D**oS / orphans | Agent/MCP processes leak or wedge. | Supervisor spawn/kill/reap with restart policy; PTY drain avoids the ConPTY hang; MCP lifecycle is managed (`mcp`). | ✅ |
| **Spoofing** | A rogue MCP server impersonates a trusted one. | MCP servers are declared per-agent manifest (trusted like the manifest, §4); no auto-discovery of arbitrary servers. | ✅ |

## 7. Host-mediated network egress (key verification)

The sidecar has no network client of its own. The single outbound path is the host performing a
**host-chosen** provider key-check on the sidecar's behalf (`host.verify_key`, CPE-347) — the sidecar
sends only `{provider, key}` and never a URL.

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **E**oP / SSRF | A sidecar coerces the host into fetching an attacker-chosen URL (SSRF, port-scan, cloud-metadata endpoint). | The host maps `provider` → a URL from a fixed **allow-list** (OpenRouter/OpenAI/Anthropic key-check endpoints); it never accepts a URL from the sidecar. `host.verify_key` is a narrow key-check, so **no `Capability::Network`/`network.fetch` general-fetch primitive exists** to abuse. | ✅ |
| **I**nfo disclosure | The API key leaks in transit or to the wrong host. | Sent over TLS (rustls) only to the allow-listed endpoint, in the provider's standard auth header; never logged (`Redactor` unchanged) and never echoed back in the verdict. | ✅ |
| **T**ampering / spoofing | A MITM returns a forged "valid" to get a bad key stored. | The verdict is fail-safe: only a definitive 401/403 yields a live rejection; any **inconclusive** result (transport error, unexpected status, rate-limit) is reported `live:false` and is never upgraded to "verified", so a forged/failed response cannot produce a false green. Default rustls cert validation resists MITM. | ✅ |
| **D**oS | A slow/hostile endpoint wedges host servicing. | 12s request timeout; the call runs on the per-sidecar servicing thread, so a stall is contained to that sidecar's capability servicing, not the explorer. | ✅ |
| **Repudiation / privacy** | The check reveals the user is validating a key. | Only the key's own provider is contacted, only on an explicit "Check" click — no third party, no telemetry. | ✅ |

## 8. Explicit non-goals / accepted risks

- **The agent itself is trusted-by-user.** The platform's job is to isolate the *sidecar
  and secrets*, surface risk, and require consent — not to sandbox a coding agent the user
  deliberately runs on their own repo. A hard OS-sandbox of the agent (seccomp/AppContainer)
  is a future hardening, not a v1 promise.
- **Not sidecar-to-sidecar orchestration** — no cross-sidecar channel exists to attack
  (ADR 0001).

## 9. Verification of the required invariants

| Invariant (from CPE-304) | Result |
|--------------------------|--------|
| No plaintext secrets at rest | ✅ Windows (keychain). 🟡 macOS/Linux use in-memory (no disk, no persistence) until CPE-322. |
| No secret in logs / UI | ✅ `Redactor` + secrets never delivered to the webview. |
| No cross-sidecar reach | ✅ Per-process isolation; namespaced storage/secrets; no cross-sidecar channel. |
| No unconsented code execution | ✅ Capabilities are consent-gated by the interactive sheet (CPE-296, done) with per-capability grant/deny + revoke (CPE-274); manifest execution is limited to bundled first-party signed manifests (CPE-295) — untrusted-manifest loading is not exposed in the shipped console. |
| No UI escape to explorer | ✅ Sandboxed iframe; frame runs on its own loopback origin (≠ host origin), so cross-origin policy blocks host access even with `allow-same-origin` (CPE-334). |
| No SSRF / arbitrary network egress from a sidecar | ✅ Egress is only ever host-mediated and allow-listed: the key check to a provider endpoint (§7, CPE-347), the catalog fetch to the app's GitHub Releases (§4, CPE-376), catalog **version enumeration** via the GitHub Releases **API** (`api.github.com`, host-built URL, read-only public GET, CPE-383), and a **version-specific** catalog fetch from `releases/download/<tag>/` where the tag is validated against a strict `[A-Za-z0-9._+-]` charset so it can't escape the releases path (CPE-383). The sidecar can never supply a URL; no general fetch primitive exists. |

## 10. Gaps → tickets (sign-off blockers)

- ~~**CPE-296** — capability consent UX.~~ **DONE** (2026-07-13): interactive consent sheet with
  per-capability grant/deny + risk badges, re-prompt on newly-requested capabilities, and revoke in
  the manager (CPE-274). Enforcement is broker-side (`decide_grants` = requested ∩ consented ∩
  policy) and unit-tested (deny-secrets → no access). No longer a blocker.
- **CPE-322** — macOS/Linux **OS-keychain** backends. Until this ships, secrets don't
  persist in a native store off-Windows, so the sidecar release stays **Windows-first**.
  **Blocks cross-OS production sign-off** (Windows-only sign-off is not blocked by this).

The consent gate is closed; the remaining sign-off blocker is **CPE-322** (cross-OS). When it's
Done, re-run this review and record final sign-off in `docs/adr/0001-sidecar-platform.md`.

## 11. Sign-off record

| Scope | Decision | Date | Basis |
|-------|----------|------|-------|
| **Windows-first** (shipping scope: bundled first-party manifests, Windows keychain) | **Signed off** | 2026-07-14 | CPE-304 final review pass — every §1–§7 mitigation verified against current code; Windows keychain round-trip verified (CPE-322 log); Windows runtime QA done (CPE-382). Invariants §9 all ✅ on Windows. |
| **macOS / Linux** | **Deferred** | — | Keychain backends coded + CI-compile-verified (CPE-322) but not yet runtime-QA'd on real hardware. Gap tracked as **CPE-322** (Blocked, `needs-macos-linux`). Re-run this review and add a row here when the round-trip passes on each OS. |

Reviewer: CPE-304 review process. This records the **engineering security-review** sign-off for the
Windows-first scope; promoting the sidecar channel to a **public** cross-OS release additionally
requires the CPE-322 hardware QA above.
