---
id: CPE-1411
title: "Security: adversarial panic battery for archive.rs readers (zip/tar/7z/iso/gzip listing + extraction)"
type: Task
status: Backlog
priority: Medium
component: Backend
tags: [ready]
epic: CPE-705
created: 2026-08-07
---

## Problem (untrusted-parser scout, top likely-real-bug pick)
`crates/server/src/archive.rs`'s archive readers — `zip_entries`, `tar_entries`, `sevenz_entries`,
`iso_entries`, `gzip_single_entry`, and the matching `extract_*`/`extract_archive_entry_any` — are in NEITHER
panic harness (`parser_panic_safety.rs`, `binary_data_preview_panic_safety.rs`). Only ONE happy-path zip fixture
(`sample_fixtures.rs:142`) exercises them. This is the "user opens a file" untrusted-input class; `sevenz-rust`
(v0.6) and `iso9660` (v0.1) are young/low-scrutiny 3rd-party crates over attacker-controlled bytes. A malicious
`.7z`/`.iso`/`.tar`/`.gz` double-clicked to preview is a plausible DoS (panic/stack-overflow/OOM) vector.

## Fix direction
Extend `crates/server/tests/binary_data_preview_panic_safety.rs` (its `run_battery` pattern, per `pe_info`) with
a case per format for BOTH listing (`zip_entries`/`tar_entries`/`sevenz_entries`/`iso_entries`/`gzip_single_entry`
via the public `read_archive_entries` entry if that's the seam) AND extraction (`extract_archive_entry_any`, with
a temp `dest`). Feed realistic minimal headers/magic + fuzzed tails, truncation, huge/negative length fields,
deep nesting, garbage. Assert NEVER panics (Ok/Err, no unwind/overflow). If a real panic/overflow/OOM is found,
STOP and REPORT it as a real bug (a small bounded-guard fix is OK if obviously correct — note prominently).
`cargo test -p cpe-server` (relevant tests) + `cargo clippy --all-targets -- -D warnings` must pass (local
`os error 225` = Defender, not a fail; CI 3-OS authoritative).
