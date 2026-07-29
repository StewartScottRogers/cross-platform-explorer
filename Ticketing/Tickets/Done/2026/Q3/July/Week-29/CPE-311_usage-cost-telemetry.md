---
id: CPE-311
title: Usage/cost tracking & opt-in telemetry
type: Feature
status: Done
priority: Low
component: Multiple
tags: [ready]
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-15
---

## Summary

Running agents against paid providers costs money and tokens. Optionally surface
per-session usage/cost so the user isn't surprised. Any product telemetry is
strictly opt-in and privacy-preserving — never prompts, code, or secrets.

## Acceptance Criteria

- [x] Where the provider exposes it, show per-session token/cost usage in the
      console; aggregate per agent/provider.
- [x] Product telemetry (if any) is opt-in, documented, and contains no repo
      contents, prompts, or secret values.
- [x] A clear off switch; off means no outbound telemetry at all.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-285]], [[CPE-292]]. **Phase:** C6. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.

## Work Log
2026-07-15 — Moved to Deferred (needs-decision). Deferred-on: awaiting a product decision — whether to build telemetry at all, opt-in model, and exactly what is collected. Revisit-when: when you decide the telemetry scope/opt-in policy. Nothing external gates it — it's pickable the moment the decision is made.
2026-07-15 (user: "build usage display") — Un-deferred and built. Estimate 2-3h.

## Resolution
Built the per-session usage/cost display; **no telemetry is collected or sent** — the only outbound
channel would be none, which trivially satisfies AC2/AC3.

- **New `sidecar/ai-console/src/usage.rs`** — a `UsageScanner` that taps the session's output stream
  (read-only, mirroring the CPE-405 read-tap): parses provider-reported figures — a `Total cost: $X`
  line and `<n> input`/`input: <n>` / `<n> output` token counts (with `k`/`m` + thousands-separator
  handling), keeping the **max** per metric within a session (robust to a running total being
  reprinted). Degrades silently on unrecognized output. **9 unit tests.**
- **`console.rs`** — the session reader thread feeds each output chunk to the scanner and stores the
  running `Usage` on the `Session`; `/api/sessions` now returns per-session `usage` plus
  `usageByAgent` / `usageByProvider` aggregates (summed across sessions). Console test extended to
  assert the usage shape.
- **`launcher.html`** — a per-tab usage badge + a status-bar readout (`#sb-usage`) for the active
  session, refreshed by a 5 s poll of `/api/sessions`. `fmtUsage` shows cost first (`$0.123`), then
  tokens (`2.0k tok`), and blanks when nothing was reported (CSS hides the empty badge). **2 launcher
  jsdom tests.**
- **AC2/AC3 — no outbound telemetry.** Nothing is transmitted anywhere; the readout is purely a
  display of what the provider itself printed in the terminal. The status-bar tooltip states
  "nothing is sent anywhere". "Off = no outbound telemetry" is the default and only state.

Verified: `cargo test --lib` 181 passed; `cargo clippy --all-targets -D warnings` clean; 30 launcher
tests pass; `npm run check` clean.
