---
id: CPE-1251
title: "Vault security review: threat-model + crypto-design doc + adversarial review + external-audit flag"
type: Task
priority: Medium
component: docs
tags: [ready, security-sensitive]
estimate: 2h
created: 2026-08-02
epic: CPE-738
closed:
---

## Context
Fifth/final slice of the encrypted-vaults half of CPE-738. Satisfies the DoD bullet "Crypto passes a
security review; keys are stored in the OS keychain, never plaintext" — done HONESTLY. A crew adversarial
review was performed incrementally across CPE-1247/1248/1249 (an independent security reviewer scrutinised
the crypto core + lifecycle + destructive paths, finding and getting fixed: a colon-filename seal/extract
asymmetry, a dest-inside-shredded-folder data-loss, a verify-writes-plaintext-to-%TEMP% leak, a
lock-drops-mapping-before-wipe strand, a re-unlock plaintext orphan, and a seal⟺extract symmetry
regression). This ticket consolidates that into a written security-review artifact + an explicit
external-audit recommendation.

## Deliverable
`docs/design/VAULT-SECURITY.md` — threat model, crypto design, key handling, security properties, honest
limits/caveats, the adversarial review record, and an explicit "professional external crypto audit
recommended before GA" flag. (User-facing summary already ships in `src/docs/20-vaults.md` from CPE-1250.)

## Definition of Done
- The doc accurately reflects the SHIPPED implementation (age 0.12.1 passphrase mode; `.cpevault` format;
  keyring v3; session-dir mount tradeoff; verify-before-shred; seal⟺extract symmetry).
- Honest about every known limit (plaintext in session dir while unlocked; orphaned sessions on crash →
  CPE-1252 sweep; forgotten passphrase = unrecoverable; secure-delete best-effort on SSD/CoW; age
  transitive-dep weight; single-Zero session wipe).
- Explicitly states the crew review de-risks but does NOT substitute for a professional external audit,
  which is recommended before shipping to real users at GA.
- Independently sanity-checked by the security reviewer for accuracy.

## Done 2026-08-02 (sprint) — docs/design/VAULT-SECURITY.md finalized @ main
Threat model + crypto design + key handling + honest limits + adversarial-review record + explicit
"professional external audit recommended before GA" flag. Independently accuracy-checked against the merged
code by the security reviewer: verdict DOC ACCURATE except one performance overstatement (claimed
streaming/low-memory; v1 actually buffers the whole tree in RAM) — CORRECTED (§3 fixed + §5 buffering
caveat added). No security-property overclaim found; crypto/key/path-safety/destructive-guard claims all
match the code. DoD "crypto passes a security review" met at the crew level; external audit is the GA gate.
