---
id: CPE-1418
title: "JWT preview decoder (header/payload/claims) — backend + tests"
type: Feature
status: Backlog
priority: High
component: Backend
tags: [ready]
epic: CPE-1417
created: 2026-08-07
---
## Scope
Add `jwt_preview(token: &str) -> JwtPreview` to `crates/server` (new `jwt_preview.rs`; keep SEPARATE from the
security-crate verifier — this is a VIEWER, no secret verification). Split the 3 segments, base64url-decode
header + payload, parse JSON, pretty-print. Surface: alg/typ/kid (header); all payload claims; humanize
exp/iat/nbf (unix -> ISO + "expired"/"not-yet-valid" flags); signature_present + byte length; a graceful typed
"malformed" result (never panic — add to the panic-safety battery). REUSE existing base64 + serde_json (no new
dep). specta::Type on the output + regen bindings. Unit tests: valid HS256, expired, alg:none, 2-segment,
garbage, huge. Wire into the panic-safety battery.
