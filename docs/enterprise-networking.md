# Enterprise networking — proxy, offline & air-gapped (CPE-310)

How the Agent Deck sidecar behaves on real-world corporate networks: behind an HTTP(S)
proxy, when offline, and in a locked-down / air-gapped deployment. Every outbound path is
either **proxy-aware** or **local-only**, and a single switch (`CPE_OFFLINE`) turns off all
remote calls. This is a reference for operators; the mechanisms are implemented and unit-tested
across CPE-347/369/376/308.

## The outbound surface (there are only three)

The sidecar has **no network client of its own**. Everything that touches the network is one of:

| Path | What it is | Proxy | Offline switch | Secret safety |
|------|-----------|-------|----------------|---------------|
| **Provider key check** (`host.verify_key`, CPE-347/369) | One host-mediated HTTPS GET to a **fixed 3-endpoint allow-list** (openrouter/openai/anthropic) to confirm a pasted key. | ✅ honours `HTTPS_PROXY`/`ALL_PROXY`/`NO_PROXY` (`resolve_proxy`) | ✅ `CPE_OFFLINE` → no call, reports "not checked" (never a failed save) | ✅ HTTPS **CONNECT tunnel** — the proxy sees only the hostname, never the auth header |
| **Catalog fetch** (`do_fetch_catalog`, CPE-376) | Host-built HTTPS GETs to the app's own GitHub Releases for signed agent-manifest bundles. Manual *and* auto-update both funnel through this one function. | ✅ same `resolve_proxy` (`catalog_http_get`) | ✅ `CPE_OFFLINE` checked **first** — returns `offline:true`, makes no call | ✅ carries no secret; URLs built host-side (no SSRF) |
| **Package-manager installs** (`npm`/`uv`/… via the lifecycle runner) | Installing an agent CLI shells out to its package manager. | ✅ **inherited** — the child process inherits the app's full environment (no `env_clear`), so `HTTPS_PROXY`/`NO_PROXY`/registry config flow straight through | Governed by the user action (installs are explicit, never background) | ✅ no app secret is injected into installers |

**Local-only (never proxied, by design):** LM Studio discovery/inference is a direct
`TcpStream` to `127.0.0.1` (`lmstudio::RealProbe`). Loopback must never traverse a corporate
proxy, so this path deliberately connects directly regardless of proxy settings.

## Proxy configuration

Proxy selection follows the **de-facto curl convention** (`src-tauri/src/keyverify.rs::resolve_proxy`):

1. `NO_PROXY` / `no_proxy` — comma/space list; an entry matches by exact host, by domain suffix
   (`openai.com` covers `api.openai.com`), or `*` for everything. A match ⇒ **connect directly**.
2. Otherwise the first set of `HTTPS_PROXY` / `https_proxy` / `ALL_PROXY` / `all_proxy` selects the proxy.
3. Nothing set ⇒ direct connection.

Set these however your environment normally does (system env, MDM/policy, or the shell that
launches the app). No app-specific proxy config exists or is needed — that is the point.

> **Key-in-transit:** because HTTPS to the provider is tunnelled through a `CONNECT`, an
> intercepting proxy sees only `api.openai.com:443`, not the `Authorization` header. A
> TLS-terminating (MITM) proxy would need its CA trusted by the OS like any other HTTPS client.

## Offline & air-gapped mode

Set **`CPE_OFFLINE`** to a truthy value (`1`, `true`, `yes`, `on`) to put the sidecar in an
air-gapped posture. It is the single switch enterprise deployments set via system environment or
policy. Effects:

- **No remote catalog.** `do_fetch_catalog` short-circuits before any network call and reports
  `offline:true`; the **last-known-good** catalog (bundled first-party manifests + any previously
  applied, signature-verified updates) keeps working unchanged.
- **No provider key check.** `verify_key` returns *"Offline mode — key not checked with the
  provider."* — a **format** check still runs, so the user is never blocked from saving a key.
- **No surprise outbound calls.** The only two automatic/remote paths are the two above; both are
  gated. Installs remain available but are always explicit user actions.

**Installing from local sources (air-gapped):** point the package managers at your internal
mirrors the normal way (e.g. `npm config set registry`, `UV_INDEX_URL`, a private `PATH`) — the
installer inherits that configuration from the app's environment. Already-installed agent CLIs are
local binaries and run with no network dependency.

## Offline error behaviour

Offline is a **clear, non-blocking state**, never a hard failure (CPE-299 alignment): the key
check says it couldn't reach the provider and falls back to the format check; a catalog refresh
reports `offline` and leaves the working catalog in place. Nothing silently breaks or hangs
(20 s / 12 s timeouts bound the catalog / key-check calls).

## Secrets never leak to a proxy or its logs

- Provider keys travel only inside the HTTPS CONNECT tunnel (opaque to the proxy).
- The catalog path carries no secret.
- Installers receive no injected app secret.
- All host-side logging runs through `Redactor` (`sidecar/host/src/observability.rs`), which
  scrubs secret-shaped values, so even a verbose diagnostic log can't spill a key.

## Verification

- Pure proxy/offline/`NO_PROXY` resolution is unit-tested in `src-tauri/src/keyverify.rs`
  (`resolve_proxy`, `host_matches_no_proxy`, `is_offline`, `host_of`).
- Catalog offline-gating and last-known-good are covered by the host catalog tests
  (`sidecar/host/src/catalog.rs`, incl. `a_bad_index_signature_touches_nothing_last_known_good`).

## Optional future enhancement

A **GUI toggle** for `CPE_OFFLINE` (surfacing the air-gapped mode in Settings rather than only as
an environment/policy switch) is a possible nicety, not required for enterprise operation where the
mode is set by IT. Not currently planned; file a ticket if there is demand.
