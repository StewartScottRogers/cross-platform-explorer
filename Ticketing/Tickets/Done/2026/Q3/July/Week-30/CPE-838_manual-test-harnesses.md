---
id: CPE-838
title: Manual-test harnesses (runnable examples) for the headless backend features
type: chore
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
The features built in the 2026-07-21 dayshift (native metadata bridge, instant-search query core, folder
templates, network transport loop) are headless backend code not yet wired into the GUI, so there was no
hands-on way to try them. Added **runnable `cargo` examples** that exercise each end-to-end, plus a guide
(`docs/MANUAL-TESTS.md`) explaining how to run them and independently verify the results with native OS
tools.

## Acceptance Criteria
- [x] `cpe-server` examples: `native_tags_demo` (push/pull tags to real native metadata),
      `search_demo` (parse + rank a query over a folder tree), `template_demo` (capture + stamp a folder).
- [x] `cpe-net` examples: `client` (drive `cpe-server-ref` over TCP) + `security_demo` (allow vs
      default-deny on the wire).
- [x] `docs/MANUAL-TESTS.md` with per-feature run commands, expected output, and OS-native verification
      (ADS via `Get-Item -Stream`, xattr via `getfattr`/`xattr`).
- [x] `cargo clippy --all-targets -D warnings` clean on both crates (examples are linted by CI); every
      example was actually run.

## Resolution
Added five examples:
- `crates/server/examples/native_tags_demo.rs` — builds a `TagStore` for a file, `native_bridge::push`es
  it to the OS metadata, `pull`s it back, and prints the exact OS command to inspect the stream/xattr.
- `crates/server/examples/search_demo.rs` — walks a folder, runs `index_query::{parse,rank}`, prints the
  parsed query + ranked matches.
- `crates/server/examples/template_demo.rs` — `capture`s a folder, prints the template JSON, `stamp`s it
  into a dest with `key=value` token substitution.
- `crates/net/examples/client.rs` — a `Client(Rust)` that connects to `cpe-server-ref` and `list_dir`s
  over the socket.
- `crates/net/examples/security_demo.rs` — same request against a local vs a default-deny chain, showing
  the boundary refusal.
- `docs/MANUAL-TESTS.md` — the walkthrough.

Verified: all five run; the NTFS ADS (`cpe.tags`) was confirmed present via `Get-Item -Stream *` with the
base `:$DATA` untouched; the capture→stamp round-trip produced a substituted tree on disk; the client
listed a directory over TCP (negotiated contract 1.0); the deny chain refused with `Unauthenticated`.

**Bonus:** running `template_demo` on a source file containing code (`fn main() { println!("{name}"); }`)
surfaced a substitution bug — fixed under [[CPE-839]] before finalising, so the harness demonstrates
correct behaviour.

## Work Log
- 2026-07-21 — Built the five examples + guide; ran each to prove the instructions; independently verified
  the ADS with native Windows tooling. Found + fixed CPE-839 (token-in-braces) mid-way. clippy
  `--all-targets` clean on both crates. Closing.
