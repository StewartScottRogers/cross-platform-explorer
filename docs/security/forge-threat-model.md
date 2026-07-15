# Forge (Repositories) sidecar — threat model & security review (CPE-440)

**Scope:** the **repos** sidecar (CPE-429) — the tenant that connects to and interacts with any
source-code forge (GitHub, GitLab, Bitbucket, Gitea/Forgejo, self-hosted, …): **browse** a remote
tree, **clone** to disk, and **two-way sync** (pull/push). This review covers only the surfaces the
forge tenant *adds* on top of the platform; the shared sidecar boundary (IPC, capability broker,
secrets broker, manifest trust, embedded UI/CSP, process isolation) is already reviewed and
Windows-first signed-off in [`threat-model.md`](threat-model.md) (CPE-304) — those mitigations apply
unchanged and are not repeated here.

**Method:** STRIDE per new surface, run at **design stage** so the invariants below are pinned
*before* the egress/clone/credential code (CPE-433/434/436/439) lands. **Status legend:**
✅ implemented & tested · 🟡 partial/gated · ⛔ not yet built (invariant is a build requirement).
Re-run per the [`sidecar-review-checklist.md`](sidecar-review-checklist.md).

> **Sign-off status: NOT SIGNED OFF — design-stage review.** No forge egress, clone, or credential
> code ships yet; this document defines the required invariants so those slices are *built to satisfy
> them*. Sign-off (and the ADR 0001 record, CPE-440 AC3) is gated on a **runtime-verifiable vertical
> slice** — browse+clone+credential+UI (CPE-433/434/436/439/435) demonstrated end-to-end against the
> mitigations below. Until then every row marked ✅ describes an invariant already true in reusable
> shared code; 🟡/⛔ rows are requirements on the not-yet-built forge code.

## What the forge tenant changes vs. the AI Console (CPE-304)

The AI Console's single outbound path is a **narrow, host-chosen key-check** to one of three fixed
endpoints (`threat-model.md` §7). The forge tenant is fundamentally broader and therefore riskier:

1. **Many egress hosts, some user-supplied.** ~14 providers (`sidecar/repos/providers/*.json`), and
   self-hosted kinds (GitHub Enterprise, GitLab self-managed, Gitea/Forgejo, generic-git) take a
   **user-entered host** — a much larger SSRF surface than three fixed URLs.
2. **A general read surface** (browse arbitrary repos/paths), not a single boolean key-check.
3. **Untrusted content lands on the user's disk** (clone/pull), where the AI Console only ran a
   user-chosen agent.
4. **An outbound *write* path** (push / two-way sync) — exfiltration and destructive-history risk
   the AI Console simply doesn't have.

## Assets & trust boundaries (delta)

- **New assets:** forge credentials (personal access tokens, OAuth tokens, SSH private keys); the
  contents cloned to disk; the integrity/history of the user's **local** repos when syncing.
- **New boundaries:** (f) host ⇄ **many** external forge APIs on the sidecar's behalf (allow-listed,
  §A); (g) remote forge content ⇄ the user's filesystem (clone/pull writes untrusted bytes, §C);
  (h) the user's local repo ⇄ a remote (push writes local bytes outward, §D).
- **New adversaries:** a malicious/compromised forge host or a hostile self-hosted URL; a malicious
  **repository** (crafted to attack `git` or the filesystem on clone); a network MITM/proxy; a
  mistake that pushes secrets to the wrong remote.

## A. Host-brokered forge API egress (browse / metadata) — CPE-433

The sidecar has **no network client of its own** (per ADR 0001 / CPE-304 §7). Forge API calls are
**method-based**, not URL-based: the sidecar asks e.g. `forge.browse{provider, connection, repo,
ref, path}`; the **host** builds the URL from the provider manifest's `api_hosts` + validated path
components and performs the call. The sidecar never supplies a URL.

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **E**oP / SSRF | Sidecar coerces the host into fetching an attacker-chosen URL (cloud-metadata `169.254.169.254`, internal services, port scan). | No general-fetch primitive: the sidecar sends `{provider, method, params}`, never a URL. Host maps `provider` → allow-listed host from the manifest (`ProviderRegistry::egress_allow_list()` — the union of every provider's `api_hosts`) and refuses any host not on it. Path/query built from **validated** params only (owner/repo/ref/path against a strict charset; no `..`, no scheme, no `@`, no CR/LF header injection). | ⛔ CPE-433 (allow-list data ✅ in `providers.rs`; host broker not built) |
| **E**oP / SSRF (self-hosted) | A user-entered self-hosted host (`github-enterprise`, `gitlab`, `gitea`, `generic-git`) points at an internal/link-local address or is DNS-rebound after allow-listing. | The self-hosted host is pinned **per connection** and added to the allow-list only for that connection. The host **resolves and rejects** private/loopback/link-local/ULA ranges and the cloud-metadata IP before connecting, and re-checks post-resolution (anti-rebinding). HTTPS only; redirects that leave the pinned host are refused. | ⛔ CPE-433 |
| **I**nfo disclosure | A token leaks in transit / to the wrong host / in logs. | TLS (rustls) only, token in the provider's standard auth header, sent **only** to the allow-listed host for that provider; `Redactor` (CPE-304 §1/§3) scrubs it from all logs; never echoed in browse results (§B). | 🟡 (Redactor ✅ shared; forge wiring ⛔) |
| **T**ampering / spoofing | MITM forges an API response. | Default rustls cert validation; results are treated as **untrusted data** and parsed defensively (§B) — a forged listing can mislead the *view* but cannot execute or escape (no code runs from a browse). | 🟡 |
| **D**oS | A slow/hostile forge wedges host servicing. | Per-request timeout (reuse the 12s pattern from `keyverify`), bounded response size, and the call runs on the per-sidecar servicing thread so a stall is contained to that sidecar (CPE-304 §1/§7). | ⛔ CPE-433 |

## B. Parsing untrusted API responses (browse) — CPE-434

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **T**ampering | Malformed/hostile JSON crashes or misleads the browser. | `parse_github_contents` (and every future provider parser) is **pure** and total: malformed/unexpected JSON yields an **empty** list, never a panic; entries are plain `{name, path, is_dir, size}` data with no executable meaning. Unit-tested. | ✅ (`browse.rs`) |
| **I**nfo disclosure / path confusion | A crafted `path` in a response (`../`, absolute, UNC) later drives a write outside the target. | Browse entries are display-only; any subsequent **clone/checkout** goes through git into a consented target dir (§C), not by trusting API-supplied paths. Path components used to build the *next* API call are re-validated host-side (§A). | 🟡 (parser ✅; clone consumer ⛔) |

## C. Clone / pull brings untrusted content to disk — CPE-436

A cloned repository is **attacker-controlled data**. Writing it to disk is the forge tenant's most
dangerous act and is treated like the AI Console's "untrusted executable content" stance (CPE-304
§4/§6): **surface it, consent it, never auto-run it.**

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **E**oP (code execution on clone) | A repo runs commands during clone/checkout via hooks, `.gitattributes` clean/smudge filters, `ext::`/`file::` transports, or `fsmonitor`. | Clone with hardening flags: `-c core.hooksPath=` (empty ⇒ no hooks), `-c protocol.ext.allow=never -c protocol.file.allow=never`, `-c core.fsmonitor=false`, `-c core.symlinks=` guarded on Windows, and **no filter execution** (`-c filter.*.process=` unset / `GIT_ALLOW_PROTOCOL` pinned to `https:ssh:git`). Submodules are **not** recursed by default (`--recurse-submodules=no`) to avoid submodule-URL injection. | ⛔ CPE-436 |
| **Tampering** (path traversal) | Crafted paths/symlinks write outside the target directory. | Clone only into an **explicit, user-chosen empty target dir** (host-validated absolute path, must be inside a user-approved root; refuse `.git` nesting/overwrite). Git itself rejects `..`/absolute entries in trees; symlink following is constrained to the worktree. | ⛔ CPE-436 |
| **Consent** | User doesn't realise they're materialising untrusted content locally. | Cloning requires **explicit consent** naming the source + target ("this downloads untrusted content to `<path>`; it will not be executed"). Mirrors the manifest untrusted-content consent (CPE-296). | ⛔ CPE-436 (+ CPE-435 UI) |
| **D**oS (disk) | Zip-bomb / huge repo fills the disk. | Optional `--depth`/size guidance surfaced; the clone is a child process (own kill domain) and cancellable; failures clean up the partial target. | ⛔ CPE-436 |
| **Non-execution** | The cloned code is later run by the app. | The forge tenant **never executes** cloned content. Running it (e.g. via the AI Console on that folder) is a separate, explicitly user-initiated act with its own consent. | ✅ (design invariant) |

## D. Two-way sync / push (outbound write) — reuses CPE-438 planner

Push is the forge tenant's unique **write-outward** path: exfiltration (private code/secrets to the
wrong remote) and destructive history (force-push) are the risks.

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **I**nfo disclosure (exfiltration) | Local content — possibly secrets — is pushed to an attacker/wrong remote. | Push targets **only** the connection's own configured remote (host-pinned, §A); no sidecar-supplied push URL. The sync **planner is safe-by-default** (CPE-438): it plans a push only when the local branch is ahead of *its own* upstream, warns on a dirty tree, and surfaces the remote identity for confirmation. Optional pre-push secret-scan is a future hardening. | 🟡 (planner ✅ `sync.rs`; execution/consent ⛔) |
| **Tampering / destruction** | A force-push rewrites/erases remote history. | The planner **never force-pushes** unless `allow_force` is explicitly set, and on divergence it prefers merge/rebase-then-push or **blocks** (`DivergePolicy::Manual`) rather than clobbering — verified by `plan_sync` unit tests. | ✅ (`sync.rs`, planner); ⛔ (execution) |
| **Repudiation** | User can't tell what a sync will do before it happens. | Sync is **plan → confirm → apply**: `SyncPlan` (actions, `conflicts_possible`, warnings, `blocked`) is shown before anything runs. | 🟡 (plan ✅; UI/apply ⛔) |

## E. Offline & corporate proxy (reuse CPE-369/310)

The forge tenant reuses the existing, unit-tested egress plumbing rather than inventing its own:
`is_offline` (honours `CPE_OFFLINE`), `resolve_proxy` (`HTTPS_PROXY`/`ALL_PROXY`, curl convention),
and `host_matches_no_proxy` (`NO_PROXY`) in `src-tauri/src/keyverify.rs` (CPE-369).

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| Availability / correctness | Offline/air-gapped runs make failing calls that look like errors. | Offline ⇒ **no outbound forge call at all**; browse/clone/sync report a clear "offline" state, never a failed-network error (mirrors `keyverify` `live:false`). | 🟡 (helpers ✅; forge callers ⛔) |
| **I**nfo disclosure (proxy) | A corporate proxy sees credentials, or internal forges get proxied out. | Both the API broker (§A) **and git** honour `HTTPS_PROXY`/`NO_PROXY` (git via `http.proxy`); HTTPS is tunnelled through `CONNECT`, so the proxy sees only the host, not the auth header. `NO_PROXY` keeps internal/self-hosted forges off the public proxy. | 🟡 (helpers ✅; git/broker wiring ⛔) |

## F. Explicit non-goals / accepted risks

- **`git` and `ssh` are trusted binaries.** The tenant shells out to the system git/ssh (CPE-429 D1)
  and does not sandbox them; hardening flags (§C) reduce, not eliminate, a hostile-repo surface.
- **No content malware scanning by default.** The tenant treats cloned content as untrusted and
  *surfaces + consents* it (§C); it does not promise to scan repository contents for malware.
- **Self-hosted host trust is user-asserted.** The user vouches for a self-hosted forge URL; the
  host still enforces the private-IP/metadata and TLS guards (§A).
- **SSH private keys prefer delegation.** Where possible the tenant delegates to the system
  `ssh-agent`/`git` credential machinery rather than reading raw private keys into the sidecar.

## G. Required invariants (build requirements for the vertical slice)

| Invariant | Owner slice | State |
|-----------|-------------|-------|
| No sidecar-supplied URL ever reaches the network; egress host ∈ provider allow-list (∪ per-connection self-hosted host). | CPE-433 | ⛔ (allow-list data ✅) |
| Private/loopback/link-local/metadata IPs refused even for user self-hosted hosts; anti-rebinding re-check. | CPE-433 | ⛔ |
| Browse parsing is total & non-executing on hostile input. | CPE-434 | ✅ |
| Clone runs no repo-supplied code (hooks/filters/alt-transports off), writes only inside a consented target. | CPE-436 | ⛔ |
| Clone/pull requires explicit "untrusted content to disk" consent. | CPE-436 / CPE-435 | ⛔ |
| Push only to the connection's own remote; never force-push without explicit opt-in; plan→confirm→apply. | CPE-438 / CPE-436 | 🟡 (planner ✅) |
| Tokens/keys: keychain at rest, Redactor in logs, per-sidecar namespace, never in `.git/config`. | CPE-439 | 🟡 (secrets broker ✅ shared) |
| Offline makes no call; proxy/NO_PROXY honoured by broker **and** git. | CPE-433/436 | 🟡 (helpers ✅) |

## H. Gaps → tickets (sign-off blockers)

- **CPE-433** — host-brokered, allow-listed forge egress (§A). The core SSRF mitigation; nothing
  browses/clones safely until this exists. **Blocks sign-off.**
- **CPE-436** — clone hardening + consented target (§C). **Blocks sign-off.**
- **CPE-439** — forge credentials via the secrets broker + login (§A/§G). **Blocks sign-off.**
- **CPE-435** — the consent/confirm surfaces live in the left-pane UI (§C/§D). **Blocks sign-off.**
- **CPE-322** (inherited) — macOS/Linux OS-keychain runtime QA still gates *cross-OS* credential
  persistence, exactly as for the AI Console.

## I. Sign-off record

| Scope | Decision | Date | Basis |
|-------|----------|------|-------|
| **Forge tenant (browse/clone/sync)** | **Not signed off — design-stage** | 2026-07-15 | This review pins the invariants (§G) before the egress/clone/credential code. Sign-off requires a runtime-verifiable vertical slice (CPE-433/434/436/439/435) demonstrated against §A–§E, after which a row is added here and the outcome recorded in [`docs/adr/0001-sidecar-platform.md`](../adr/0001-sidecar-platform.md) (CPE-440 AC3). |

Reviewer: CPE-440 design-stage review, extending CPE-304. When the vertical slice is verifiable,
re-run this document via `sidecar-review-checklist.md`, flip §G to ✅, and record ADR sign-off.
