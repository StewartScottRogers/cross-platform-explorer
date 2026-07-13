# Sidecar platform — Phase-0 de-risking findings (CPE-294)

The Phase-0 spike asked whether the risky technical assumptions behind the sidecar
platform hold *before* the contract was frozen. Rather than a throwaway prototype, the
questions were answered by building the real, tested components — so the evidence below is
shipped code with cross-OS CI (`.github/workflows/ci.yml` `sidecar` job) and real-process
E2E tests, which is stronger than a spike. Each finding fed [[CPE-259]] (ADR) and
[[CPE-262]] (contract).

## 1. IPC transport — **Decision: newline-delimited JSON `Envelope`s over stdio**

A JSON-line framing (`sidecar-contract::codec`) over the child's stdin/stdout gives
low-latency streaming and natural backpressure (bounded `sync_channel`,
`supervisor::IPC_CHANNEL_CAPACITY`), and is the simplest thing that is portable to all
three OSes with no socket/permission story. Length-prefixed CBOR was considered and
rejected: JSON lines are debuggable, and message sizes here (control + line-oriented PTY
output) don't justify the opacity. Streaming PTY/install output rides `Message::Event`
frames.

- **Gotcha:** every writer must flush after each frame or the host blocks on a buffered
  line; the reference sidecars and the codec do this, and the README calls it out.
- **Evidence:** `sidecar/contract/src/lib.rs` (`codec`, `Envelope`, `ENVELOPE_SCHEMA_VERSION`),
  `sidecar/host/tests/supervisor_e2e.rs`.

## 2. PTY — **Confirmed: `portable-pty` drives a real interactive CLI cross-OS**

`ai-console::pty::PtySession` spawns an agent CLI through a real PTY (Windows ConPTY, Unix
pty) with resize support.

- **Gotcha (Windows):** ConPTY never signals EOF the way a Unix pty does, so a naive
  `read_to_string` blocks forever. Resolved with a timed drain (background reader thread +
  `recv_timeout` deadline) — see the pty tests.
- **Evidence:** `sidecar/ai-console/src/pty.rs` and its tests.

## 3. UI embedding — **Decision: sidecar serves its own UI on loopback; host embeds a sandboxed iframe**

Each sidecar serves a local HTTP UI and announces `ui:<loopback-url>` via a `Status`
event; the host mounts it in an `<iframe sandbox="allow-scripts allow-forms">` — no
`allow-same-origin`, so the frame is an opaque origin that cannot reach explorer
internals. No CSP change was needed (the app CSP is `null`); `parseUiAnnouncement` accepts
loopback-only URLs.

- **Gotcha:** Tauri v2 has no first-class "embed a sidecar view" primitive; the
  loopback-iframe pattern is the portable mechanism and keeps the trust boundary explicit.
- **Evidence:** `sidecar/ai-console/src/ui.rs`, `src/lib/components/SidecarPane.svelte`,
  `src/lib/sidecar.ts`; **visually verified end-to-end** on Windows (CPE-318/319).

## 4. Keychain — **Partial: `keyring` verified on Windows; macOS/Linux deferred**

The secrets capability is backed by the OS keychain via `keyring`, verified with a real
round-trip against **Windows Credential Manager**.

- **Gotcha:** the crate needs a per-OS native backend feature; only `windows-native` is
  enabled today (`sidecar/host/Cargo.toml` gates `keyring` to `cfg(windows)`). Linux
  (Secret Service/libsecret) is unavailable headless in CI without a session bus, and
  macOS needs its own feature. Until those land the provider falls back to an in-memory
  backend on non-Windows — which is why the sidecar **release** channel is Windows-only.
- **Follow-up:** [[CPE-322]] adds the macOS/Linux backends + cross-OS release matrix.
- **Evidence:** `sidecar/host/src/providers/secrets.rs`.

## 5. Process supervision — **Confirmed: clean spawn/kill/reap, no orphans**

`sidecar/host/src/supervisor.rs` spawns the child, completes a token-authenticated
handshake, and reaps it on drop; a restart policy and crash-injection E2E cover failure
paths. Each sidecar is its own OS process = its own crash domain.

- **Evidence:** `supervisor.rs`, `tests/restart_e2e.rs`, `tests/supervisor_e2e.rs`.

## Net

All five assumptions hold; the one caveat (non-Windows keychains) is scoped and tracked.
The contract was frozen on this evidence and has evolved additively to v1.2. No throwaway
prototype remains — the production components subsume it.
