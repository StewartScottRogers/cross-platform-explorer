---
title: JWT & Certificate Preview
order: 26
category: Previews & Media
categoryOrder: 6
---

# JWT & Certificate Preview

Select a JSON Web Token or a certificate-family file and the preview pane decodes it automatically —
no dialog to open, no tool to launch. Both viewers are **read-only decoders, not verifiers**: they show
you what a file contains, but never check a signature, a trust chain, or anything cryptographic.

## JWT preview

Opens automatically for `.jwt` and `.jws` files. The pane shows:

- **Header** — algorithm (`alg`), type (`typ`), and key ID (`kid`) when present.
- **Validity** — the `iat`/`nbf`/`exp` claims as human-readable dates. An **EXPIRED** badge appears
  when `exp` is in the past; a **NOT YET VALID** badge appears when `nbf` is in the future.
- **Signature** — whether a signature segment is present and its decoded length in bytes, or a note
  that the token is unsigned (`alg: none`, or a malformed signature segment).
- **Claims** — the full payload, pretty-printed and wrapped, with a one-click **Copy** button.
- **Raw header** — the full header JSON, also copyable.

A malformed token (wrong number of segments, invalid base64, invalid JSON) still shows whatever part
*could* be decoded — a broken payload doesn't hide a perfectly readable header — alongside a clear error
line describing what failed.

**This is a viewer, not a verifier.** Nothing here checks the signature against a key, so a token
shown here as "valid-looking" could still be forged. Never treat this preview as proof a token is
trustworthy.

## Certificate preview

Opens automatically for `.pem`, `.crt`, `.cer`, `.der`, `.csr`, `.pub`, and `.key` files. The backend
auto-detects the actual shape from the file's content (not just its extension), decoding whichever of
these it finds:

- **X.509 certificate** (PEM-armored or raw DER) — subject, issuer, serial, version, validity window
  (with **EXPIRED**/**NOT YET VALID** badges), signature algorithm, public-key algorithm + size/curve,
  subject alternative names (as reflowing pills), CA flag, key-usage and extended-key-usage flags, and
  SHA-256/SHA-1 fingerprints (monospace, each with its own Copy button).
- **PKCS#10 certificate signing request (CSR)** — the requested subject, requested SANs, and the
  request's public key.
- **Standalone public key** — algorithm and size/curve.
- **Private key file** — algorithm and size **only**. The actual key material is never read by the
  decoder, let alone shown — the same guarantee the backend gives for any private-key input.

A file that isn't recognizable as any of the four shapes (or is corrupted) shows a clear error message
instead of guessing.

## Read-only, never a trust decision

Neither viewer performs cryptographic verification of any kind — no signature check, no chain
validation, no revocation check, no "this is safe" verdict. They exist purely so you can inspect a
token or certificate's contents without leaving the file explorer or reaching for an external tool.
