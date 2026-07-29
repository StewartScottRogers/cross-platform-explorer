---
id: CPE-999
title: SimHash near-duplicate text/document detection
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-997
---

# CPE-999 — SimHash near-duplicate text/document detection

## Summary

Extends epic CPE-997 ("Near-duplicate & similar-image detection") beyond images: a Charikar SimHash
text fingerprint plus a `near_duplicate_docs` convenience that reuses [`crate::perceptual::cluster`] —
the same union-find, single-link, Hamming-distance clustering core that CPE-998 built for images — so
near-identical notes/READMEs/text files can be grouped with the same primitive.

New module `crates/server/src/simhash.rs`. Pure std, no new dependencies, no filesystem I/O (a caller
reads file bytes/text and feeds `(id, text)` pairs in — the same "caller decodes, module fingerprints"
split as `perceptual::phash`).

## Design

Two free functions:

- `pub fn simhash(text: &str) -> u64` — tokenise (lowercase, split on non-alphanumeric runs, drop
  empties — the same tokenizer shape as `embedder.rs`'s bag-of-words tokenizer), hash each token with a
  stable 64-bit FNV-1a (offset basis `0xcbf29ce484222325`, prime `0x100000001b3` — the 64-bit sibling of
  `embedder.rs`'s 32-bit `fnv1a`; deliberately **not** `std::DefaultHasher`, which is randomly seeded per
  process and would make the same document hash differently between runs). Maintain a `[i64; 64]`
  accumulator: for each token, for each bit position, `+1` if that bit of the token hash is set, `-1`
  otherwise (a repeated token naturally gets more weight, no separate term-frequency step). Final bit `i`
  is `1` iff `accumulator[i] > 0`. Empty/tokenless text falls out to `0` automatically (an all-zero
  accumulator has no slot `> 0`) — not a special case in the code.
- `pub fn near_duplicate_docs(docs: &[(String, String)], max_distance: u32) -> Vec<Vec<String>>` —
  `simhash`es each `(id, text)`, then hands the `(id, u64)` pairs straight to `crate::perceptual::cluster`.
  This is the payoff of CPE-998's design: the clustering core didn't need to know it would one day get
  text fingerprints instead of image dHashes.

`pub mod simhash;` added to `crates/server/src/lib.rs` after `perceptual`, with a doc comment describing
it as the text-fingerprint sibling.

## Acceptance Criteria

- [x] `simhash` tokenises + hashes deterministically; identical text hashes identically (Hamming
  distance 0 via `crate::perceptual::hamming`).
- [x] Near-identical text (a paragraph vs. the same paragraph with one word changed + one sentence
  added) produces a small Hamming distance; an unrelated document produces a meaningfully larger one,
  with a real, non-marginal gap between the two.
- [x] Empty / whitespace-only text hashes to `0`.
- [x] `near_duplicate_docs` groups a near-identical pair while dropping an unrelated singleton;
  deterministic; input-order-independent; threshold (`max_distance`) governs grouping (tight threshold →
  no groups, loose threshold → transitive single-link merges everything).
- [x] A stable-hash guard test pins `fnv1a64` against published FNV-1a-64 test vectors and pins one
  `simhash` output against a hard-coded constant, so an accidental hash/bit-order change fails CI.
- [x] Zero new dependencies; pure, no filesystem I/O.
- [x] `pub mod simhash;` declared in `lib.rs` with a doc comment.
- [x] `cargo test --lib simhash` passes (10 tests).
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo clippy --all-targets --features index -- -D warnings` clean.

## Work Log

- 2026-07-24 — Built `simhash.rs` end-to-end: `simhash` (Charikar SimHash via 64-slot signed-vote
  accumulator over per-token 64-bit FNV-1a hashes) and `near_duplicate_docs` (thin wrapper around
  `crate::perceptual::cluster`, reusing CPE-998's clustering core unchanged).
  - **Tokenisation choice:** copied `embedder.rs`'s tokenizer shape exactly (lowercase, split on
    non-alphanumeric runs, drop empties) rather than inventing a new one — keeps the two "turn text into
    features" modules in this crate behaviourally consistent (e.g. the same casing/punctuation handling),
    even though `embedder::tokenize` itself isn't `pub` so a local copy was needed.
  - **Hash choice:** 64-bit FNV-1a (offset basis `0xcbf29ce484222325`, prime `0x100000001b3`) — the
    64-bit sibling of `embedder.rs`'s 32-bit `fnv1a`, chosen for the same reason: it's *stable*
    (deterministic across runs/processes), unlike `std::collections::hash_map::DefaultHasher` which is
    randomly seeded per-process and would break SimHash's entire premise of a fingerprint that's
    comparable across calls. Verified against the published canonical FNV-1a-64 test vectors for `""`,
    `"a"`, `"foobar"`.
  - **Threshold/fixture assumption:** measured on this ticket's paragraph-length fixtures (a ~40-word
    near-identical pair differing by one word + one appended sentence, vs. a topically unrelated
    paragraph of similar length), the near pair lands at Hamming distance **2** and the unrelated pair at
    **15** (out of 64 max) — a real ~13-bit gap. Test assertions use headroom around those measured
    values (`near <= 8`, `far >= 12`, gap `>= 6`) rather than pinning the exact numbers, so the suite
    stays robust to minor fixture wording changes without being tautological. Chose realistic
    multi-sentence fixtures deliberately — SimHash's "similar text → small distance" property is noisy on
    very short strings (too few tokens for the 64-slot vote to average out); a sentence-or-two-length
    fixture is where the property holds robustly.
  - **`max_distance` default (documented here since, like `perceptual::cluster`, it's a caller-supplied
    parameter, not hard-coded):** no single fixed default is prescribed by this ticket since real-world
    documents vary far more in length/vocabulary than the fixed-size image dHash CPE-998 tuned for, but
    based on the measured fixtures here, a threshold in the **~8–12 bit** range is a reasonable starting
    point for a future caller (permissive enough for lightly-edited near-duplicates, tight enough to stay
    well clear of the unrelated-document distance observed) — left as a caller parameter, same as
    `perceptual::cluster`, so it can be tuned per document corpus or exposed as a sensitivity control.
  - **Stable-hash guard:** pinned `fnv1a64` against the published FNV-1a-64 reference vectors, and pinned
    `simhash("hello world")` to `0x0410_d846_0008_8803`, computed once by running the test with the
    assertion removed and reading back the actual value, then hard-coding it — so a future accidental
    change to tokenisation, hash algorithm, or bit-packing order fails CI immediately instead of silently
    invalidating any persisted SimHash fingerprints.
  - Verified (PowerShell, `crates/server`, `$env:PATH += ";$env:USERPROFILE\.cargo\bin"`):
    `cargo test --lib simhash` → 10/10 passed; `cargo clippy --all-targets -- -D warnings` clean;
    `cargo clippy --all-targets --features index -- -D warnings` clean. Also re-ran
    `cargo test --lib perceptual` (11/11 still passing) to confirm CPE-998's module was untouched.
  - Status → Done; ACs checked; moving to `Tickets/Done/2026/Q3/July/Week-30/`.
