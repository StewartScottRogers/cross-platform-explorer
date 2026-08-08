---
id: CPE-1471
title: "Sidecar stdout reader uses unbounded buf.lines() → a compromised AI-Console sidecar OOMs the host (prod-reachable)"
type: Bug
status: Backlog
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
