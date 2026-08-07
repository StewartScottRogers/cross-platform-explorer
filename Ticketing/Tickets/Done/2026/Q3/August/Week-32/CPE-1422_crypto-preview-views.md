---
id: CPE-1422
title: "Frontend: preview-pane views for .jwt and certificate files (auto-decode on open)"
type: Feature
status: Done
priority: High
component: Frontend
tags: [ready]
epic: CPE-1417
created: 2026-08-07
closed: 2026-08-07
---
## Scope
Wire the shipped `commands.jwtPreview(path)` (CPE-1418) + `commands.certDecode(path)` (CPE-1419) into the
PreviewPane so opening a crypto file shows a decoded panel:
- `.jwt` (and `.jws`) → header (alg/typ/kid) + pretty-printed payload/claims, `exp`/`iat`/`nbf` as human dates
  with an **expired / not-yet-valid badge**, and a signature-present + length indicator. Make it clear this is a
  VIEWER (it does not verify the signature).
- `.pem` / `.crt` / `.cer` / `.der` / `.csr` / `.pub`/public-key → cert detail: subject, issuer, serial, validity
  (human + expired badge), key algo+size/curve, SANs, key-usage/EKU, is_ca, SHA-256/SHA-1 fingerprints. CSR shows
  requested subject/SANs. A private-key file shows ONLY algorithm + size (never secret material — backend already
  guarantees this).
Register the extensions in the preview registry / preview-kind detection. Build a `CryptoPreview.svelte` (or two
small views) that render the `JwtPreview`/`CertPreview` structs cleanly (reuse existing preview styling; wrap long
values, mono for fingerprints, reflow pills for SANs per the tick-tack rule). Add jsdom render-specs (mock the
commands, assert the decoded fields render + the expired badge). Add a `src/docs/*.md` page + `sectionDocs.ts`
entry (CPE-579). Exercise against `samples/crypto/*`. No backend change (consume the existing bindings).

## Implementation

- `src/lib/preview/provider.ts` — two new `PreviewKind`s, `"jwt"` and `"cert"`, each with their own extension
  set (`JWT_EXT` = jwt/jws; `CERT_EXT` = pem/crt/cer/der/csr/pub/key), inserted before the generic text
  provider (pem/crt/cer/csr categorise as "code" text in `filetypes.ts` and would otherwise be claimed by it;
  jwt/jws have no category today and would otherwise fall to the hex last-resort).
- `src/lib/components/JwtPreview.svelte` (new) — self-contained view in the DataBrowser.svelte style (fetches
  its own data from a `path` prop via `commands.jwtPreview`, no prop-drilled callback). Renders header
  (alg/typ/kid), a Validity section (iat/nbf/exp as human dates via the existing `formatDate` util, with
  EXPIRED/NOT YET VALID pill badges driven by the backend's `expired`/`not_yet_valid` flags), a Signature
  section (present + byte length, or "unsigned"), pretty-printed Claims + raw header JSON (mono, wrapped, each
  with a Copy button), and an always-visible "viewer, not a verifier" banner. A malformed token still renders
  whatever fields decoded, plus the backend's `error` string.
- `src/lib/components/CertPreview.svelte` (new) — same shape, wired to `commands.certDecode`. Branches on the
  result's `kind` (`certificate` / `csr` / `public_key` / `private_key`) to show the matching fields: full
  certificate detail with SAN/key-usage/EKU as reflowing pills (tick-tack rule) and mono SHA-256/SHA-1
  fingerprints (each with a Copy button); CSR's requested subject + SANs; a standalone public key's algo/size/
  curve; and — critical — a private key branch that renders ONLY algorithm + size behind its own explicit
  "key material is never read" banner, matching the backend's guarantee.
- `src/lib/components/PreviewPane.svelte` — imports both new components and adds
  `provider.kind === "jwt"` / `"cert"` branches next to the existing `DataBrowser`/`HexView` branches.
- `src/docs/26-crypto-preview.md` (new) + `src/lib/sectionDocs.ts` — new `"crypto-preview"` `Section` →
  `26-crypto-preview` doc-slug entry (cross-cutting feature inside the preview pane, not a sidebar view —
  same treatment as `native-metadata`/`terminal`/`vaults`/`file-health`).
- `src/lib/components/JwtPreview.test.ts` / `CertPreview.test.ts` (new) — jsdom render-specs mocking
  `../bindings.gen`'s `commands.jwtPreview`/`commands.certDecode` (DataBrowser.test.ts's recipe), covering a
  valid HS256 token, an expired token (EXPIRED badge), an `alg: none` unsigned token, a malformed-token decode
  error, an invoke-level load error, and the clipboard-copy affordance; and an RSA cert, an expired EC cert
  (EXPIRED badge + curve), a CSR, a standalone public key, a private-key file (asserts key material never
  renders), a decode error, and a load error.

## Verification

- `npm run check` — 0 errors, 0 warnings.
- `npx vitest run` — 218 test files / 2410 tests, all green (includes the 15 new render-specs +
  `sectionDocs.test.ts`'s exhaustiveness guard).

## Work Log

- 2026-08-07 — Implemented both preview views + provider-registry wiring + docs + tests; full suite green.
  PR opened against `main`.
