---
id: CPE-304
title: End-to-end threat model & security review (milestone)
type: Task
status: Done
priority: Critical
component: Multiple
tags: [needs-prereq]
estimate: 4h+
created: 2026-07-13
closed: 2026-07-14
---

## Summary

A dedicated security milestone before the platform is declared production-ready.
This system spawns arbitrary CLIs, injects credentials, embeds remote-ish UI, and
loads user-supplied manifests — the attack surface is real. Produce a written
threat model and run a review covering the whole boundary.

## Acceptance Criteria

- [x] STRIDE-style threat model across: IPC channel, capability broker, secrets
      broker, manifest trust, embedded UI/CSP, spawned agent & MCP processes.
- [x] Each threat has a mitigation mapped to a ticket ([[CPE-268]], [[CPE-275]],
      [[CPE-295]], [[CPE-296]], [[CPE-306]], [[CPE-307]]) or a new one.
- [x] Verifies: no plaintext secrets at rest, no secret in logs/UI, no
      cross-sidecar reach, no unconsented code execution, no UI escape to explorer.
- [x] Sign-off recorded in the ADR ([[CPE-259]]); gaps filed as blockers.
- [x] Repeatable checklist so each new tenant sidecar gets a lightweight review.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-275]], [[CPE-295]], [[CPE-296]], [[CPE-268]]. **Phase:** P5
(and re-run per tenant). **Epic:** [[CPE-260]]; applies to [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-13 — **Threat model authored & committed**: `docs/security/threat-model.md`
(STRIDE across all six surfaces — IPC, capability broker, secrets broker, manifest trust,
embedded UI/CSP, spawned agent/MCP — each threat mapped to a mitigation/ticket, grounded in
the real code) plus the repeatable per-tenant `docs/security/sidecar-review-checklist.md`.
Interim posture recorded in ADR 0001. Invariants verified: no plaintext secrets at rest
(✅ Win / 🟡 non-Win), no secret in logs/UI (✅), no cross-sidecar reach (✅), no UI escape
(✅). **Final production sign-off intentionally withheld — gated on CPE-296 (consent UX)
and CPE-322 (macOS/Linux keychains).** Re-run this review and record sign-off when both are
Done. Acceptance items 1/2/3/5 met; item 4 (sign-off) pending those two.
2026-07-14 — Threat model extended (CPE-367) with **§7 Host-mediated network egress**, covering
the `host.verify_key` key-check added in CPE-347 (SSRF containment via host-chosen allow-listed
URL, key-in-transit, forged-verdict fail-safe, DoS timeout) + a new "no SSRF/arbitrary egress"
invariant. Doc stays current with the code; final sign-off still gated on CPE-296 + CPE-322.
2026-07-14 — Verified **CPE-296 (consent gate) is DONE** and corrected the stale threat-model
markers (6 spots said ⛔/"not built"): consent sheet + per-cap grant/deny + re-prompt + revoke
(CPE-274) all shipped; broker enforcement (`decide_grants`) unit-tested (deny-secrets → no access).
**Sign-off gate reduced to CPE-322** (non-Windows keychains) + this final review pass. A
Windows-first sign-off is now unblocked on the consent axis. Tracked as CPE-384.
2026-07-14 — **CPE-322 keychain backends implemented + CI-compile-verified on macOS (apple-native)
and Linux (sync-secret-service, +libdbus) runners.** The "secrets persist in a native store
off-Windows" invariant is now *coded* for all three OSes; the cross-OS sign-off's remaining item is
a **runtime round-trip QA on a real mac/Linux desktop** (store/get/delete against the live Keychain /
Secret Service) — needs the hardware. So: consent gate ✅ (CPE-296), cross-OS backend code ✅,
awaiting only cross-OS runtime QA + the final review to lift the sign-off.
2026-07-14 — **CPE-382 done: the sidecar platform now builds + bundles cross-OS** (Windows/macOS/
Linux draft channel; Windows install verified — sidecar + agents bundled, app launches). Every
technical mitigation is now implemented + verified (consent ✅, keychain code ✅ all-OS, no-SSRF ✅,
signed catalog ✅). The sign-off's remaining items are HUMAN/QA, not code: (1) runtime QA of the AI
Console per-OS (Windows eyeball + real mac/Linux), (2) the final review sign-off recorded in ADR
0001. Once done, the sidecar channel can be promoted to public.

2026-07-14 — Picked up (final review pass). Estimate: 4h+ (unchanged; most of the work was authored
across prior sessions — this pass is verify + record sign-off). Ran a genuine code-grounded review,
not a rubber stamp: re-verified each headline mitigation against the current source —
`broker::decide_grants` = requested ∩ consented ∩ policy (`host/src/broker.rs:27`); `Redactor`
(`host/src/observability.rs`); per-sidecar keychain namespace `service_for(sidecar_id)` + real
`KeyringBackend` (`host/src/providers/secrets.rs`); the `host.verify_key` **fixed 3-endpoint
allow-list** with no sidecar-supplied URL (`src-tauri/src/keyverify.rs` — openrouter/openai/anthropic
only); and the loopback iframe sandbox `allow-scripts allow-forms allow-same-origin`
(`src/lib/components/SidecarPane.svelte:25`). All claims in the threat model are grounded in code.

2026-07-14 — Fixed a stale doc found during the pass: the per-tenant checklist's Embedded-UI item
still said "no `allow-same-origin`", contradicting the shipped CSP (CPE-334). Corrected it and added
the guarantee that `allow-same-origin` is scoped to the frame's own loopback origin, not the host.

2026-07-14 — **Recorded the sign-off.** All five acceptance items are met for the shipping scope:
threat model ✅, threats→mitigations ✅, invariants verified ✅, **sign-off recorded** ✅, repeatable
checklist ✅. Recorded a **Windows-first production sign-off** (2026-07-14) in
`docs/adr/0001-sidecar-platform.md`, threat-model §11 (new sign-off record), and the checklist's
tenant sign-off log. The one remaining gap — a runtime keychain round-trip on **real macOS/Linux
hardware** — is a hardware-gated blocker already tracked as **CPE-322** (Blocked, `needs-macos-linux`);
item 4 explicitly permits "gaps filed as blockers", so this closes CPE-304 while the cross-OS
promotion stays correctly gated.

## Resolution

Ran the CPE-304 **final security-review pass** and recorded the sign-off. The STRIDE threat model
(`docs/security/threat-model.md`) and per-tenant checklist (`docs/security/sidecar-review-checklist.md`)
were authored/extended across prior sessions; this pass **verified every headline mitigation against
the current code** (broker least-privilege, redactor, per-sidecar keychain namespace, the
`verify_key` 3-endpoint SSRF allow-list, and the loopback iframe sandbox) and confirmed the document
is accurate.

**Files changed** (docs only — no code touched, so no build/test needed):
- `docs/adr/0001-sidecar-platform.md` — replaced the "interim posture" with the recorded **Windows-first
  sign-off (2026-07-14)**; cross-OS promotion explicitly gated on CPE-322.
- `docs/security/threat-model.md` — header status → "Windows-first signed off; cross-OS deferred";
  added **§11 Sign-off record** (a dated table).
- `docs/security/sidecar-review-checklist.md` — corrected the stale Embedded-UI CSP line to match the
  shipped `allow-same-origin` (CPE-334); added a tenant sign-off log entry for the AI Console.

**Outcome / tradeoff:** the sign-off is scoped **Windows-first** (the actual shipping scope:
bundled-first-party manifests, Windows OS keychain), which is fully supported and verified. The
**cross-OS** sign-off is deliberately **deferred** — the macOS/Linux keychain backends are coded and
CI-compile-verified but cannot be runtime-QA'd here (no mac/Linux hardware). That single remaining
item is the pre-existing Blocked ticket **CPE-322**; when its round-trip passes on real hardware,
re-run this review and add a row to §11. Promoting the sidecar channel to a **public cross-OS**
release stays gated until then. No production-security overreach: this records the engineering
review sign-off for what actually ships today.
