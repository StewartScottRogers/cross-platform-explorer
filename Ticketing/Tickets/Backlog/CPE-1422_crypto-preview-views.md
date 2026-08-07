---
id: CPE-1422
title: "Frontend: preview-pane views for .jwt and certificate files (auto-decode on open)"
type: Feature
status: Backlog
priority: High
component: Frontend
tags: [ready]
epic: CPE-1417
created: 2026-08-07
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
