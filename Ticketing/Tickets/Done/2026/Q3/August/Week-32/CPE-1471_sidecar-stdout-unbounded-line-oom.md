---
id: CPE-1471
title: "Sidecar stdout reader uses unbounded buf.lines() → a compromised AI-Console sidecar OOMs the host (prod-reachable)"
type: Bug
status: Done
priority: Medium
component: Backend
tags: [ready, security]
epic: CPE-862
created: 2026-08-08
---
## Vector (found in the crypto/IPC deep audit, 2026-08-08) — the one PROD-REACHABLE finding of that sweep
`sidecar/host/src/supervisor.rs:~232-233`: the reader thread does `BufReader::new(stdout); for line in buf.lines()`.
`BufRead::lines()` grows a `String` until `\n` or EOF with NO byte cap. The AI-Console sidecar child runs arbitrary
agent/model-provider code (explicitly in the threat model). A compromised/malicious sidecar writes gigabytes to
stdout WITHOUT a newline → `read_line` allocates until the HOST process OOM-aborts. `IPC_CHANNEL_CAPACITY` bounds
the COUNT of buffered envelopes, not the size of a single line, so it does not help.

## Reachability
PROD-REACHABLE — the sidecar-platform (AI Console) build is the one that always ships/installs. This reader backs
the live `serve_ai_console_requests` loop (`src-tauri/src/lib.rs:~7618`), the `ui:` announcement loop (`~8853`),
and the agent-board connection. Impact = full-app denial of service (host process killed). Not RCE/disclosure (the
child is already a separate process), but a real live DoS.

## Fix direction
Replace `.lines()` with a manual `read_until(b'\n', &mut buf)` (or a `.take(CAP)`-bounded reader) capped at e.g.
8–16 MiB; on overflow, send `Err("frame too large")` on the channel and stop the reader — the supervisor already
treats a read error as connection loss and restarts with capped backoff, so the failure mode is graceful. Add a
test that a >CAP line without a newline yields a bounded error, not unbounded allocation.

## Effort / blast radius
S / one reader loop in supervisor.rs. Epic CPE-862 (sidecar reliability). Serialize with CPE-1472 (same file).

## Work Log

- 2026-08-08 — Fixed.

  Replaced `BufReader::new(stdout).lines()` with a hand-written `read_bounded_line()` helper that reads in
  `BufReader`-chunk sizes via `fill_buf`/`consume` and bails the instant appending the next chunk would push
  the buffered line past a cap — so a line with no `\n` anywhere is never buffered in full, unlike
  `BufRead::lines()`/`read_until` which both grow unboundedly until the delimiter or EOF.

  **Cap chosen: 16 MiB** (`MAX_LINE_BYTES` in `supervisor.rs`). Justification: every message on this channel
  (`Request`/`Response`/`Event`/etc., CPE-270) carries small control/status/tool-result JSON in practice, and
  16 MiB matches the cap the app already enforces at its *other* large-payload IPC boundary —
  `ai_console::http::MAX_REQUEST_BODY` (16 MiB, the AI Console's own HTTP server). A real message that large
  would already be rejected downstream, so 16 MiB comfortably exceeds any legitimate envelope while still
  bounding a hostile/corrupt one to a fixed, small multiple of what a compromised sidecar could force the
  host to allocate before the connection is severed.

  On overflow the reader sends one `Err("frame too large: line exceeds the {cap}-byte cap")` on the channel
  and stops — exactly the existing "read error → connection loss" path, which the supervisor already handles
  with a capped-backoff restart (`RestartPolicy`). No panic, no host-level effect beyond that one sidecar's
  connection.

  Added `read_bounded_line` unit tests: a >cap line with no newline errors at the cap without buffering it
  all (checked at both a small test cap and the real 16 MiB `MAX_LINE_BYTES`), a normal `\n`-terminated line
  under the cap still reads correctly, and a final unterminated line at EOF is handled like `BufRead::lines()`
  used to (returned as the last line, then a clean `Ok(0)` next call). All existing e2e reader tests
  (`supervisor_e2e.rs`, `restart_e2e.rs`, `hello_sidecar_e2e.rs`) still pass unchanged, confirming normal
  framed traffic is unaffected.

  **Verification:**
  - `cargo build` (sidecar/host) — clean.
  - `cargo clippy --all-targets -- -D warnings` (sidecar/host) — clean.
  - `cargo test` (sidecar/host) — 102 passed, 1 ignored (real-OS-keychain test); all e2e + new bounded-reader
    tests green.

  PR: branch `cpe-1471-sidecar-ipc-hardening`, bundled with CPE-1472/CPE-1473 (same file cluster).
