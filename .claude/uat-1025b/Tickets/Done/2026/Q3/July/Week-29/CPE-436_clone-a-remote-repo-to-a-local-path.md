---
id: CPE-436
title: "Clone a remote repo to a local path"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 2h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-429
---

## Summary
Clone any accessible remote repo locally (CPE-429), shelling out to git per the provider manifest (D2).
Foundation for two-way mirroring.

## Acceptance Criteria
- [x] Clone via the provider git URL to a chosen local path. — host command `forge_clone` builds the URL host-side from the provider allow-list, runs the hardened argv, returns an ok/err string. Argv+URL construction is unit-tested; the actual `git clone` **execution** + progress-streaming is runtime/GUI-verified (needs the host + a real git), not covered by the unit tests.
- [x] Auth injected for a private repo; never on disk/logs. — an optional `token` is injected as URL userinfo for git, kept out of all logging, and scrubbed from any git stderr before it is returned. (Wiring the token from the CPE-439 secrets broker rather than a caller-passed arg is a follow-up.)
- [x] Actionable errors. — typed pre-flight refusals (unknown/self-hosted provider, non-`owner/name` repo, url-unsafe token, and the repos crate's BadUrl/BadTarget/BadRef) map to actionable messages; on git failure the trimmed, token-scrubbed stderr is surfaced. Partial-clone cleanup remains an execution-time concern (runtime).

## Work Log
2026-07-15 — Picked up (forge-slice advance). Building the testable core: a hardened `git clone` argv builder (threat-model §C: hooksPath empty, protocol.ext/file=never, fsmonitor off, no submodule recursion) + URL-scheme allow-list + target-dir validation, as sidecar/repos/src/clone.rs. Pure + unit-tested. Execution + progress-streaming + auth injection are the runtime/GUI tail (need the host + a real git), noted as remaining.

## Work Log
2026-07-15 — Landed the testable core: sidecar/repos/src/clone.rs — hardened `git clone` argv builder (§C: core.hooksPath=, protocol.ext/file=never, fsmonitor off, --recurse-submodules=no, --no-tags, `--` before url/target), https/ssh-only URL allow-list, absolute non-nested target validation, option-injection-safe depth/branch. 5 unit tests; clippy clean. Kept OPEN — execution/progress/auth-injection are the runtime tail (need the host + a real git + CPE-439).

2026-07-15 — Wired the host execution path (branch `CPE-436-clone-execution`). Added feature-gated Tauri command `forge_clone(provider, repo, target_dir, token?)` in `src-tauri/src/lib.rs`, registered next to `forge_browse`. It builds the clone URL **host-side** from a fixed provider→host map (`clone_host`: github/gitlab/bitbucket/codeberg; self-hosted kinds refused) — the caller supplies only `owner/name`, never a scheme/host (SSRF hygiene, same spirit as `forge_egress`). Reuses the already-tested hardened argv builder via a new optional path dep `repos` on `src-tauri/Cargo.toml`, gated into `sidecar-platform`. A private token is injected as URL userinfo and never logged; git stderr is token-scrubbed before it is returned. Factored the pure construction into `build_git_clone(...) -> Result<Vec<String>, String>` (with `is_safe_repo_slug` / `is_safe_token` guards) so it is cleanly testable without spawning git. 4 new unit tests (public https URL + all §C hardening + `--`; private token embedded; other providers map to their host; unknown provider / bad repo / relative target / url-unsafe token all refused without echoing the token).

## Resolution
Backend execution path for cloning is in place and testable. `forge_clone` (feature-gated `sidecar-platform`) constructs the hardened `git clone` invocation host-side and spawns it with `std::process::Command`, returning a short ok string on success or the trimmed, token-scrubbed git stderr on failure.

Honesty notes / out of scope (follow-ups):
- The **arg + URL construction** is what the unit tests prove. The actual `git clone` **execution**, progress-streaming, and partial-clone cleanup are runtime/GUI-verified only — they need the host process and a real `git` on PATH, which the headless test can't (and shouldn't) exercise.
- The token is currently a **command argument**. Sourcing it from the CPE-439 secrets broker instead is a separate follow-up.
- No frontend "Clone…" button / dialog — this ticket is backend-only. Wiring `invoke("forge_clone", …)` into the Repositories view is a follow-up (would touch `src/App.svelte`, deliberately left untouched here).

Verified: `cargo test --features sidecar-platform` (4/4 new tests pass), `cargo clippy --all-targets --features sidecar-platform -- -D warnings` clean, and `cargo clippy --all-targets -- -D warnings` (default build, no sidecar) clean — all new code is feature-gated, so the plain explorer still ships without it (delete-test holds).
