---
id: CPE-1425
title: "samples/crypto/ folder — JWT + certificate fixtures to exercise all ops + README"
type: Task
status: Backlog
priority: High
component: Repo
tags: [ready]
epic: CPE-1417
created: 2026-08-07
---
## Scope
Create a committed `samples/crypto/` folder the user opens IN THE APP to exercise JWT + cert features, generated
REPRODUCIBLY (commit a small generator, e.g. a `#[test]`/xtask/script using rcgen + a base64url+HMAC helper — do
NOT require openssl on the machine). Include:
- JWT: `hs256-valid.jwt` (rich sub/iss/aud/exp-in-future), `expired.jwt`, `alg-none.jwt`, `rich-claims.jwt`.
- Certs: `self-signed-rsa.pem` (+ `self-signed-rsa.key`), `self-signed-ec.pem`, `chain.pem` (leaf+intermediate+
  root), `cert.der`, `expired.pem`, `request.csr`, `public-key.pem`.
- `README.md`: one line per file — what it is + which operation it demonstrates.
The `.key` is a THROWAWAY demo key clearly labelled "DEMO ONLY — do not use in production". Keep the folder small.
