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
| **I**nfo disclosure (exfiltration) | Local content — possibly secrets — is pushed to an attacker/wrong remote. | Push targets **only** the connection's own configured remote (host-pinned, §A); no sidecar-supplied push URL. The sync **planner is safe-by-default** (CPE-438): it plans a push only when the local branch is ahead of *its own* upstream, warns on a dirty tree, and surfaces the remote identity for confirmation. Optional pre-push secret-scan is a future hardening. | ✅ **execution shipped (CPE-495, see §J.1)**; planner ✅ `sync.rs` |
| **Tampering / destruction** | A force-push rewrites/erases remote history. | The planner **never force-pushes** unless `allow_force` is explicitly set, and on divergence it prefers merge/rebase-then-push or **blocks** (`DivergePolicy::Manual`) rather than clobbering — verified by `plan_sync` unit tests. `forge_sync` has **no force arm at all** (§J.1). | ✅ (`sync.rs` planner **+ execution CPE-495**) |
| **Repudiation** | User can't tell what a sync will do before it happens. | Sync is **plan → confirm → apply**: `SyncPlan` (actions, `conflicts_possible`, warnings, `blocked`) is shown before anything runs. | ✅ **UI/apply shipped (CPE-495, §J.1)** |

## E. Offline & corporate proxy (reuse CPE-369/310)

The forge tenant reuses the existing, unit-tested egress plumbing rather than inventing its own:
`is_offline` (honours `CPE_OFFLINE`), `resolve_proxy` (`HTTPS_PROXY`/`ALL_PROXY`, curl convention),
and `host_matches_no_proxy` (`NO_PROXY`) in `src-tauri/src/keyverify.rs` (CPE-369).

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| Availability / correctness | Offline/air-gapped runs make failing calls that look like errors. | Offline ⇒ **no outbound forge call at all**; browse/clone/sync report a clear "offline" state, never a failed-network error (mirrors `keyverify` `live:false`). | 🟡 (helpers ✅; forge callers ⛔) |
| **I**nfo disclosure (proxy) | A corporate proxy sees credentials, or internal forges get proxied out. | Both the API broker (§A) **and git** honour `HTTPS_PROXY`/`NO_PROXY` (git via `http.proxy`); HTTPS is tunnelled through `CONNECT`, so the proxy sees only the host, not the auth header. `NO_PROXY` keeps internal/self-hosted forges off the public proxy. | 🟡 (helpers ✅; git/broker wiring ⛔) |

## J. v2 delta — push execution + Generic-Git host admission (CPE-495 / CPE-498)

v1 shipped browse + clone + credentials (native path, §I row 2). v2 turns on the two surfaces this
document flagged as the highest-risk and previously unbuilt: **push execution** (the outbound write,
§D) and **Generic-Git arbitrary-host egress** (a user-supplied clone host, §A self-hosted row). This
section records how the shipped code satisfies — and where it accepts — the §A/§D invariants.

### J.1 Push / two-way-sync execution (CPE-495)

`forge_sync{path, action}` executes exactly one planner-chosen step (`pull-ff` / `pull-merge` /
`pull-rebase` / `push`) via `git -C <path> …`; `forge_repo_status` produces the `SyncPlan` preview
first (plan → confirm → apply).

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **T**ampering / destruction | A background or UI action force-pushes and rewrites remote history. | There is **no force action** in `forge_sync` at all — the match arms are ff-pull / merge-pull / rebase-pull / plain push; `allow_force` is never surfaced to the command. A divergence under `DivergePolicy::Manual` is `blocked` in the plan, not reconciled. | ✅ (`forge_sync`, `sync.rs`) |
| **I**nfo disclosure (exfiltration) | Local secrets pushed to the wrong/attacker remote. | `push` targets only the repo's **own configured upstream** (`git push` with no URL); the sidecar/host never supplies a push URL. The plan shows the branch + ahead/behind before the user confirms. | ✅ (execution now wired; pre-push secret-scan still a future hardening) |
| **Repudiation** | User can't tell what a sync will do. | The Sync dialog renders the `SyncPlan` (planned steps, ahead/behind, dirty, `conflicts_possible`, warnings, `blocked`) and runs only on explicit confirm; each step's git output is shown, and a failure halts the sequence. | ✅ (CPE-495 UI) |
| **I**nfo disclosure (token on push) | The push credential leaks in logs / error text. | `forge_sync` runs plain `git push` — it injects **no** token into the argv and logs none; auth comes from git's own credential machinery / the remote already in `.git/config`. (Residual: see J.3.) | ✅ (no token in the push path) |

### J.2 Generic-Git arbitrary-host egress + consent admission (CPE-498)

The Generic-Git provider clones/syncs a **user-supplied** https/ssh URL, so the host is no longer
drawn from the provider allow-list. `repos::parse_remote` extracts the bare host; `forge_clone_url`
refuses it unless it is in a **consent-based egress allow-list** persisted at
`app_data/forge-admitted-hosts.json`.

| STRIDE | Threat | Mitigation | Status |
|--------|--------|-----------|--------|
| **E**oP / SSRF (coerced egress) | The frontend/a manifest coerces a clone to an attacker-chosen host without the user intending it. | A host reaches git **only** after it is in the allow-list, and it lands there **only** via `forge_admit_host`, which the UI calls after the user clicks **Grant** on a consent band **naming the exact host**. No silent admission; the list **fails closed** (unreadable ⇒ admits nothing). | ✅ |
| **E**oP / SSRF (over-broad grant) | Consenting to one host silently admits siblings/wildcards. | `forge_admit_host` stores **exactly the normalized host** and refuses `*`, `/`, or whitespace — consent to `a.example.com` never admits `b.example.com`. `parse_remote` strips userinfo, keeps the port off the host key, and normalizes case/trailing-dot so the admit key can't be spoofed. Unit-tested. | ✅ (`generic.rs`, `build_generic_clone` tests) |
| **E**oP (alt-transport / hostile URL) | A crafted URL smuggles `ext::`/`file://`/`git://` or an option-looking target. | `parse_remote` accepts only `https://`, `ssh://`, scp-like `user@host:path`; `build_generic_clone` then defers to the **same hardened `repos::build_clone_args`** as v1 (empty hooksPath, `protocol.ext/file.allow=never`, no fsmonitor, no submodule recursion, `--` before url/target). | ✅ |
| **I**nfo disclosure (token) | An https token leaks via logs or the error path. | The token is injected as userinfo only for `https` (ssh uses the agent — token ignored), never logged, and **scrubbed** (`replace(t,"***")`) from any git error before it is returned. Unit-tested (`generic_clone_rejects_bad_urls_and_unsafe_tokens`). | ✅ (in-flight/argv) — but persists at rest, see J.3 |
| **Untrusted content** | A generic remote is more likely hostile than a known forge. | Identical to §C: clone runs no repo code and writes only into the user-picked target folder (the folder dialog is the "untrusted content to disk" consent). The generic path adds **no** new write behaviour. | ✅ |

### J.3 Residual risks & accepted trade-offs (v2)

- **A token clone persists the credential in `.git/config`.** Both `forge_clone` (v1) and
  `forge_clone_url` embed an https token as URL userinfo, which git writes into the repo's
  `.git/config` remote URL. It is therefore **at rest in plaintext on the user's own disk**, inside
  their profile — the same trust domain as the checked-out working tree — and is **not** logged. This
  contradicts the aspirational §G "never in `.git/config`" invariant; the honest current state is
  *never in logs, but on disk for a token-cloned repo*. Hardening follow-up: clone without embedding
  and use a git **credential helper** so the token stays in the keychain. Tracked as a hardening note
  here rather than a sign-off blocker (accepted: local-disk, user-profile, no-log).
- **Self-hosted / private-range hosts are permitted *by design*, under consent.** Unlike the API
  broker's §A invariant (refuse private/loopback/metadata IPs), the Generic-Git clone path **allows**
  a private-LAN or self-hosted host — that is the feature's purpose. The mitigation is **informed
  consent naming the exact host** (§J.2), consistent with §F "self-hosted host trust is
  user-asserted", not IP-range blocking (which would break legitimate LAN forges). The cloud-metadata
  IP has no legitimate clone use but is still gated behind the same explicit, host-naming consent, so
  it cannot be reached without a deliberate user grant.
- **DNS anti-rebinding is not applied to git clone.** The API-broker invariant's post-resolution
  re-check (§A) is not meaningful for a `git clone` child process; the consent-per-host model is the
  boundary. Accepted for the shell-out design (§F "git is a trusted binary").

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
| **Forge v2 — push execution + Generic-Git host admission** | **Invariants implemented + unit-verified; runtime/GUI QA deferred** | 2026-07-16 | v2 turned on the two surfaces v1 deferred (§J): **push/two-way-sync execution** (`forge_sync`, CPE-495) and **Generic-Git arbitrary-host egress** (`forge_clone_url` + the consent allow-list, CPE-498). Every §D/§A invariant with code is implemented + unit-tested: `forge_sync` has **no force arm**; push uses the repo's own upstream with **no injected token**; the Generic-Git host reaches git **only** via consent-based, no-wildcard, fail-closed admission (`forge_admit_host`, `repos::parse_remote`, `build_generic_clone` tests); the same hardened clone builder + token-scrub apply. **Residual (accepted, §J.3):** token persists in `.git/config` for a token-clone (on-disk, user-profile, never logged); private-range/self-hosted hosts permitted **by design** under explicit host-naming consent (not IP-blocked). **Deferred:** live-network/GUI runtime QA + a credential-helper follow-up. |
| **Forge — implementation review (native path)** | **Invariants implemented + unit-verified; runtime QA + two-way-mirror deferred** | 2026-07-15 | The slice shipped as a **native explorer feature** (not a sidecar — see CPE-429 decision): `forge_browse`/`forge_clone`/`forge_*_token` over `forge_egress` + `RepoBrowser.svelte`. Every §G invariant that has code is now **implemented and unit-tested**: host-side allow-listed URL building + no-sidecar-URL (`forge_egress`), private/loopback/metadata IP refusal (`is_blocked_ip`), total non-executing browse parsing (`parse_github_contents`/`parse_gitlab_tree`), hardened clone args (`repos::build_clone_args` §C flags) into a user-picked target (folder dialog = the "untrusted content to disk" consent), keychain-namespaced token never logged (CPE-439), and offline/proxy honoured (reuses `keyverify` `is_offline`/`resolve_proxy`). **Deferred:** two-way **push/mirror** (planner `sync.rs` built, not wired) and **runtime/GUI QA** of live network+git — same shape as the AI Console's Windows-first sign-off with cross-OS runtime deferred (CPE-304 §11). |

Reviewer: CPE-440 design-stage review, extending CPE-304. When the vertical slice is verifiable,
re-run this document via `sidecar-review-checklist.md`, flip §G to ✅, and record ADR sign-off.
