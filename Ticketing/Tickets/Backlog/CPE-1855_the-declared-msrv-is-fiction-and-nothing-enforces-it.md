---
id: CPE-1855
title: the declared MSRV is fiction and nothing enforces it
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

Every Rust manifest in the repo declares `rust-version = "1.77.2"` — `crates/server/Cargo.toml:5`, its
eleven sibling crates, and `src-tauri`. Nothing checks it:

- no `rust-toolchain.toml`
- no MSRV job in CI
- CI uses `dtolnay/rust-toolchain@stable`, so it always builds on whatever is current

And it is already false. `ErrorKind::NotADirectory` stabilised in **1.83.0** and is used at
`crates/server/src/transfer.rs:1525` and `:1564` — inside tests, so the violation has been confined to
`cargo test`. CPE-1742 makes it a **library-build** violation for the first time by using the same API in
`fsutil.rs`'s `confined_to`.

## Why it matters, and why it is Low

Nobody is currently building on 1.77.2, so nothing is broken today. The cost is that the declaration is
**load-bearing-looking and unchecked**: a contributor reading `rust-version` reasonably believes it, and a
reviewer weighing whether an API is safe to use has no way to find out other than by looking up each
one's stabilisation version by hand — which is how this was found.

A declared constraint that nothing enforces is the same shape this repo keeps closing elsewhere: a claim
recorded as fact with no mechanism behind it.

## Acceptance criteria

- [ ] Decide, and record the reasoning: either raise the declared `rust-version` to what the code actually
      needs, or drop the declaration. Do not leave a number nobody checks.
- [ ] If a real MSRV is kept, something must enforce it — a `rust-toolchain.toml`, an MSRV CI leg, or
      `cargo-msrv`. An unenforced MSRV will drift again within a few tickets.
- [ ] Whatever is chosen must be applied across **all twelve manifests plus `src-tauri`**, not just the one
      that surfaced the problem. A partial sweep presented as complete is this repo's most-repeated defect.
- [ ] Establish what the true minimum actually is before setting a number — audit for other post-1.77 APIs
      rather than assuming `NotADirectory` is the only one. Enumeration is how the third, fourth and fifth
      instances get found on tickets like this.
- [ ] If the declaration is dropped rather than raised, say what replaces it as the answer to "what can we
      build on", so the next person does not re-add a guess.

## Notes

Found by the independent security reviewer during CPE-1742, while checking whether
`ErrorKind::NotADirectory` was safe to use in shared production containment code. It flagged this as
non-blocking for that PR and correct not to fix there — but asked that it not go unrecorded.

Related: CPE-1742 (the first library-build use), and the `transfer.rs` test-only uses that preceded it.
