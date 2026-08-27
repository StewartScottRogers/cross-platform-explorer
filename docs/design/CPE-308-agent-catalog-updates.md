# CPE-308 — Agent catalog update / subscription (design)

**Status:** draft for review. **Part 1 (CPE-371) has landed**; this document is the design for the
remaining "remote subscription" work, which carries security-weighted decisions that want sign-off
before implementation.

## Why

The platform's purpose is to keep up with the coding-agent market *without shipping an app release*
— new agents and changed install recipes should arrive as **data**. Today the agent catalog is the
manifests bundled in `ai-console/agents/`, frozen at build time. This design lets a **signed**
catalog refresh from a configured source at runtime.

## What part 1 already gives us (CPE-371)

- `catalog::verify_manifest(bytes, sig_hex, trusted_keys)` — sidecar-local ed25519 verification,
  byte- and format-compatible with the host trust engine (`sidecar_host::trust`, CPE-295).
- `AgentRegistry::load_signed_source(dir, trusted_keys)` — loads manifests from an **untrusted**
  directory only when each has a valid sibling `*.json.sig` from a trusted key; verified manifests
  override by id; **additive** so a bad/empty/unverified source never removes good agents
  (last-known-good in memory).
- Wiring: `CPE_AICONSOLE_CATALOG` (dir) + `CPE_AICONSOLE_CATALOG_KEYS` (hex keys); unset ⇒ no-op.

So the **trust gate and the merge semantics already exist**. Part 2 is: *where the signed catalog
comes from, how it's fetched, how the user controls it, and how it survives offline.*

## Goals / non-goals

**Goals:** fetch/refresh a signed catalog from a configured source; verify-before-trust; slot into
the registry with schema migration; user controls (manual refresh, auto-update toggle, pin/rollback);
offline-safe (last-known-good keeps working). **Non-goals (for now):** a public marketplace,
third-party *unsigned* catalogs (those stay consent-gated per CPE-296), server-side curation.

## Key decisions (need sign-off)

### D1 — Where verification lives (single trust authority)

We now verify signatures in **two** places: the host `TrustStore` (sidecar/agent manifests, CPE-295)
and the new sidecar-local `catalog::verify_manifest`. That risks drift.

- **Option A — Host fetches + verifies, hands the sidecar a trusted catalog.** One authority (the
  host owns keys + policy + provenance + consent, CPE-296). The sidecar never touches the network or
  crypto for the catalog; it receives already-trusted manifests (or a path to them) over a capability.
  *Recommended* — it matches ADR 0001 (crypto/keys are host concerns) and reuses the host's existing
  `TrustStore`/policy-allowlist/provenance.
- **Option B — Sidecar fetches + verifies** with keys provisioned from the host. Simpler wiring,
  but duplicates trust logic in the sidecar and spreads the key material.

**Recommendation: A.** Part 1's sidecar-local verifier still earns its keep as the *loader's* final
gate (defence in depth), but the authoritative fetch+verify is host-side.

### D2 — The source & its shape

A **signed catalog index**: one document listing entries `{id, schema_version, url|inline, sha256,
signature, version}` plus its own top-level signature. Fetched from a **configured** source URL
(first-party default; enterprise can override). Not an arbitrary user URL by default — see threat model.

### D3 — Key distribution / trust-on-first-use

First-party catalog key ships **in the app** (like the updater pubkey in `tauri.conf.json`). A
user/enterprise source key is added explicitly (config/policy), TOFU-recorded with provenance
(CPE-295 `record_provenance`) and re-prompted on change. No implicit trust of a new key.

### D4 — Auto-update default

Recommend **manual refresh by default**, auto-update **opt-in** (mirrors the app updater consent
posture and the CPE-296 "no unconsented code execution" invariant). Pin/rollback per manifest id.

## Fetch mechanism

Reuse the **host-mediated egress** built in CPE-347/369: a narrow host method (e.g.
`host.fetch_catalog`) where the **host** holds the URL (allow-listed / configured), makes the call
**proxy- and offline-aware** (`resolve_proxy` / `CPE_OFFLINE`, CPE-369), verifies signatures, and
persists a **last-known-good** copy to storage. No general fetch is exposed to the sidecar (no SSRF
primitive) — same principle as `host.verify_key`.

## Offline-safe & rollback

- **Offline / air-gapped:** the persisted last-known-good catalog loads with no network; `CPE_OFFLINE`
  disables refresh entirely (ties into CPE-310). A failed fetch never degrades the working catalog.
- **Anti-rollback:** entries carry a monotonic `version`; a fetched entry older than the installed one
  is refused unless the user explicitly rolls back. Prevents a signed-but-stale replay.
- **Where that `version` comes from (CPE-1941):** the **committer timestamp of the tagged commit**,
  computed by `.github/workflows/scripts/catalog-version.sh` from the checkout the release job
  already has — never `date +%s` at publish time. The difference is the whole of anti-rollback's
  soundness: a publish-time clock records *when the workflow ran*, so re-running the release workflow
  on an **old tag** stamped that tag's old manifests with a number newer than anything installed and
  the engine accepted them — a content downgrade with every signature, hash, and schema check intact,
  reachable with no key compromise by anyone able to re-run an Actions job. A commit timestamp is a
  property of the content: re-running an old tag reproduces that tag's own, older number, which is
  refused. Demonstrated both ways in `sidecar/host/tests/catalog_republish_downgrade.rs`; the
  workflow wiring is pinned by `src/lib/catalogPublishVersion.test.ts`.
  - *Installed-base transition:* both schemes emit Unix epochs, so the numbering is continuous.
    `CATALOG_VERSION_FLOOR` in that script is a **fatal** publish-time check that the derived version
    exceeds the highest version any client can hold, so the scheme change can never emit a number the
    installed base would reject. Measured across all 65 releases carrying a catalog index: the
    highest version on a **published** release is `1784894333` (`v0.57.31-sidecar`) — the real
    installed base, since clients fetch `releases/latest/download/`. `v0.57.32` carries a higher
    `1784951108` but is a **draft** (`published_at: null`), so its assets were never served and no
    client ever held it; the floor clears both. That sequence is monotone across all 65, so there is
    no evidence the old-tag republish was ever exercised in the wild. A repo-committed counter was
    rejected precisely here: restarting at a small integer against an installed base holding ~1.79
    billion would refuse every future release, permanently.
  - *Residual (pre-fix tags), and it is narrower than it first looks:* re-running a tag cut **before**
    this change runs that tag's own copy of `release.yml`, which still reads a clock. But the re-run
    uploads to the **old tag's own release**, and the default client fetch is
    `releases/latest/download/`, which does not resolve to it. So a stale re-run reaches the default
    update path only when that tag already *is* `latest` (republishing its own current content — no
    downgrade), or when the user drives the CPE-383 rollback picker, which fetches
    `releases/download/<tag>/` explicitly and passes `allow_downgrade` — i.e. the user opted in.
    Closing it entirely is operational, **cheapest first**: (1) restrict who may re-run workflows — a
    repo setting, and it costs installed clients nothing; (2) raise the floor above any number a
    stale re-run stamped. Rotating `CPE_CATALOG_SIGNING_KEY` is the **expensive last resort**, not
    the first move: `CATALOG_TRUSTED_KEYS` is a **compile-time** list, so rotation makes every
    already-installed client reject *every* bundle — legitimate ones included — until it app-updates,
    and the same secret signs the model snapshot (CPE-450/451), so it breaks that catalog too.
  - *Known gap the fix introduces (follow-up ticket):* the floor is a **static** ratchet, not a
    monotonic one, and nothing on the publish side compares against the **last published** version.
    A release cut from an older commit — a hotfix on a maintenance branch, or `git tag` on a non-tip
    commit — stamps a `%ct` below the live catalog's while still clearing the floor, so the release
    goes green and every client silently returns `Rollback`. `date +%s` could not do this, since
    publish order *was* version order. Ordinary releases are cut from the tip (`release.ps1` pushes
    `HEAD --tags`), so it only bites a deliberate off-tip tag.
- **Schema migration (CPE-300):** an entry with an older `schema_version` is migrated up before
  validation; unknown-future schema is skipped (as today).

## Threat-model additions (new surface — must land with part 2)

A remote catalog is **egress + supply-chain**, so `docs/security/threat-model.md` needs a section:

| STRIDE | Threat | Mitigation |
|--------|--------|-----------|
| Tampering / EoP | Malicious/tampered manifest → RCE via install/run commands. | Signature-verified against a trusted key *before* any command is eligible; unsigned/third-party stays consent-gated (CPE-296). |
| Spoofing / SSRF | Sidecar coerces a fetch to an attacker URL. | Host holds the URL (configured/allow-listed); no general fetch exposed — as with `host.verify_key`. |
| Rollback | Signed-but-stale catalog replayed to reintroduce a bad recipe. | Monotonic `version`, anti-downgrade unless explicit rollback. The `version` is derived from the **tagged commit**, not from publish time, so re-publishing an old tag cannot advance it (CPE-1941). |
| DoS | Hostile/slow source wedges startup. | Timeout + last-known-good fallback; refresh is async/off the launch path. |
| Info disclosure | Catalog fetch leaks which agents/enterprise. | First-party/configured source only; no per-user telemetry; key in transit only via TLS CONNECT (CPE-369). |

## Proposed slice breakdown (each a `ready` sub-ticket after sign-off)

1. **Catalog index schema + verifier** (host-side `TrustStore` over the index; content hashes;
   monotonic version). Pure, unit-testable.
2. **`host.fetch_catalog`** — host-mediated, proxy/offline-aware fetch + verify + persist
   last-known-good; hand the verified dir to the sidecar (feeds part 1's `load_signed_source`).
3. **User controls** — manual refresh, auto-update toggle, pin/rollback (launcher UI + storage).
4. **Threat-model section + provenance/consent wiring** (CPE-295/296).

## Open questions for sign-off

- D1 (host-authoritative fetch/verify) — agree?
- D2 source shape (a signed index doc) and the **default first-party source URL** — who hosts it?
- D4 auto-update default = **off** — agree?
- Is enterprise "point at your own signed catalog" in scope for v1, or first-party only?
