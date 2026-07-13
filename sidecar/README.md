# Sidecar platform

The sidecar platform (ADR [`docs/adr/0001-sidecar-platform.md`](../docs/adr/0001-sidecar-platform.md))
lets the Cross-Platform Explorer host large, fast-moving "Mega-Features" as **isolated
child processes** that talk to the host over one small, versioned contract.

- **`contract/`** — `sidecar-contract`: the *only* shared surface. The framed
  `Envelope`, the `Message` union, handshake, capability model, lifecycle, error
  taxonomy, and JSON-line `codec`. Both host and sidecars depend on this and on
  nothing of each other's — the one-way boundary the delete-test enforces.
- **`host/`** — `sidecar-host`: host-side platform logic (manifest registry, version
  negotiation, the capability **broker** + providers, the process **supervisor**, and
  the **conformance kit**). Pure logic, testable without the explorer.

## Build your own sidecar (start from the reference)

The reference sidecar is **[`host/src/bin/hello_sidecar.rs`](host/src/bin/hello_sidecar.rs)** —
copy it. It exercises all four brokered capabilities end-to-end and is the fixture the
E2E tests spawn as a real process
([`host/tests/hello_sidecar_e2e.rs`](host/tests/hello_sidecar_e2e.rs)). A minimal,
capability-free variant is [`host/src/bin/echo_sidecar.rs`](host/src/bin/echo_sidecar.rs).

**A sidecar is just a process that speaks JSON-line `Envelope`s over stdio.** Its source
depends on **`sidecar-contract` only** — never on `sidecar-host` internals. That is the
non-negotiable boundary (ADR 0001); CI's delete-test (CPE-272) enforces it.

The protocol shape is always:

1. **Hello** — emit `Message::Hello` declaring your id, version, `CONTRACT_VERSION`, and
   the `Capability`s you want. You get nothing you don't ask for.
2. **Welcome → Ready** — the host replies with `Message::Welcome` listing the
   capabilities actually **granted** (after user consent). Emit `Lifecycle::Ready`.
   *Only exercise capabilities that appear in `capabilities_granted`* — a host that
   grants nothing (e.g. the conformance kit) must see a passive sidecar.
3. **Use capabilities** — call one by *sending* a `Message::Request` with a distinct
   correlation id (e.g. `storage.dir`, `secrets.set`/`secrets.get`, `context.current`)
   and reading the matching `Message::Response`. Emit `Message::Event`s
   (notify/progress/status) for anything the user should see.
4. **Serve host requests** — answer inbound `Message::Request`s with a `Response`
   correlated by envelope id; return an error `Response` for unknown methods. Exit
   cleanly on `sidecar.shutdown`.

Keep the loop robust: read line-by-line, skip blank/undecodable lines, and **flush after
every write** so the host never blocks on a buffered frame.

### Capability methods the host brokers

| Capability | Method(s) | Notes |
|-----------|-----------|-------|
| `context` | `context.current` → snapshot | read-only explorer state |
| `secrets` | `secrets.set{name,value}` / `secrets.get{name}` / `secrets.delete{name}` | per-sidecar namespace |
| `storage` | `storage.dir` → `{dir}` | private per-sidecar directory |
| `events`  | (emit) `Event::Notify` / `Progress` / `Status` | host→sidecar signals via `Message::Signal` |

### Prove it conforms

Run the conformance kit against your binary as a real process — see
`conformance_kit_passes_against_hello` in the E2E test. It drives the handshake and the
core request/response behaviours and reports pass/fail.

> The fuller SDK — a helper crate plus a `cargo generate`-style scaffolder so you don't
> hand-roll the read/write loop — is tracked as **CPE-303**. Until then, copy
> `hello_sidecar.rs`.

## Running the tests

```sh
cd sidecar/host
cargo test                                   # unit + real-process E2E
cargo clippy --all-targets -- -D warnings
```

## AI Console — extending it

Add a coding agent, provider, or plugin by manifest (no code): see [ai-console/docs/adding-an-agent.md](ai-console/docs/adding-an-agent.md).

## Building the explorer WITH the sidecar platform

Sidecars are **bundled, never downloaded** (ADR 0001). The default app build is
sidecar-free (the delete-test). To build the explorer *with* the platform and the
AI Console bundled in:

```
# 1. build the sidecar release binary/binaries
(cd sidecar/ai-console && cargo build --release)
# 2. build the app with the feature + the bundle overlay
npm run tauri build -- --features sidecar-platform --config src-tauri/tauri.sidecar.conf.json
```

`src-tauri/tauri.sidecar.conf.json` is a config overlay merged in only for this build;
it ships the sidecar binary + its `sidecar.json` + the agent catalog into the app's
`sidecars/` resources, which the app resolves at runtime (no env var needed).
For dev, `npm run tauri dev -- --features sidecar-platform` with `CPE_AICONSOLE_BIN`
set to the built binary.

### Windows: Defender may block the bundled `.exe` during build

`tauri build` with the overlay copies the sidecar `.exe` into the app resources.
Windows Defender real-time protection can lock that freshly-written `.exe` mid-copy,
failing the build with `Access is denied. (os error 5)` (the `.json` manifests are
unaffected). If you hit this, add a one-time exclusion for the repo in an **elevated**
PowerShell, then rebuild:

```
Add-MpPreference -ExclusionPath "Z:\repos\cross-platform-explorer"
```

CI runners generally don't hit this. Dev (`tauri dev` + `CPE_AICONSOLE_BIN`) is
unaffected — it never copies the binary.
