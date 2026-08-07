---
id: CPE-1417
title: "EPIC: Crypto Inspector & Certificate Management (JWT + X.509)"
type: Epic
status: Done
priority: High
component: Full-stack
tags: [big-design]
created: 2026-08-07
---
## Goal (user-directed, GUI session 2026-08-07)
A useful crypto toolkit in the explorer: decode/view JWTs and X.509 certs/CSRs; CREATE keypairs + self-signed
certs; SIGN (self-sign + issue-from-CSR); all manageable from the DUAL-PANE right pane (pane-aware context menu);
plus a committed `samples/crypto/` folder to exercise every operation.

## Children
- CPE-1418 JWT preview decoder (backend)
- CPE-1419 Certificate decoder X.509 PEM/DER + CSR (backend)
- CPE-1420 Certificate create (keypair + self-signed) (backend)
- CPE-1421 Certificate sign/issue-from-CSR (backend)
- CPE-1422 Frontend preview-pane views for .jwt + cert files
- CPE-1423 Frontend cert-management dialogs (create + sign/issue)
- CPE-1424 Dual-pane right-pane cert/JWT context menu (pane-aware)
- CPE-1425 samples/crypto/ folder + README

## Decisions
New deps (pure-Rust, no OpenSSL): `x509-parser` (decode), `rcgen` (create/sign). "Signing" = cert signing
(self-sign + issue-from-CSR), NOT arbitrary file/code signing. The viewer NEVER displays private-key material
(type/size only). Demo keys in samples are throwaway + clearly labelled.

## Closed 2026-08-07
All 8 children merged (CPE-1418 JWT decode, 1419 cert/CSR decode, 1420 create, 1421 sign/issue, 1422 preview views, 1423 dialogs, 1424 pane-aware menu, 1425 samples). Crypto Inspector & Certificate Management epic COMPLETE.
