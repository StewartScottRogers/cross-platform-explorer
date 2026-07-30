---
title: "Is there any honest headless work left to build, or is the well tapped?"
date: 2026-07-29
tags: [frontier, headless, backlog-scan, cpe-server, qa-burndown, gui-smoke, unwired-engines, tapped]
status: current
---

## Question
With the backlog empty, is there any GENUINE headless slice left (pure `cpe-server`/sidecar logic, testable
with no GUI / no user resource) — the kind the CPE-1133 OGG bug was — or is the clean well tapped?

## Finding (3 independent sweeps, cross-checked vs git history, 2026-07-29): TAPPED.

**1. Correctness-marker sweep** (`TODO|FIXME|later refinement|for now|conservative|naive|not wired|assume|
best effort|simplif|HACK|XXX` across `crates/server`, `crates/{net,sftp,security,contract}`, `sidecar/*`):
every hit is a **correct-but-cautious documented tradeoff**, not a deferred bug. Verified examples: the
just-shipped CPE-1134 conservative no-attribution default (`checkpoint_store.rs`); UTF-16 BOM fallback
(`media_meta_read.rs:134`); non-crypto RNG by no-new-deps guardrail (`secure_shred.rs:16`); NOT-stack depth cap
(`query_group.rs:185`); pre-lowercased pattern (`name_search.rs:40`). The FLAC path has **no** analogue of the
OGG multi-page gap. No CPE-1133-shaped bug anywhere else.

**2. Unwired-but-built engines** (module-reachability check over all ~126 `crates/server` modules vs
`src-tauri/src/lib.rs` wiring): every genuinely-unwired module is a **documented GUI/model/attended gate**, not
an oversight — `spotlight.rs` (needs hotkey overlay window), `connections.rs` (attended connections UI +
keychain), `tray_quick.rs`/`terminal_tabs.rs`/`playlist.rs` (panel GUIs), `folder_similarity.rs`/
`dangling_links.rs`/`orphan_sidecars.rs` (CPE-1002 results need a dialog), `archive_diff.rs`/`compress_plan.rs`
(CPE-705, scoped GUI), `op_plan.rs` (needs an LLM), `column_config.rs` (column-picker UI), `media_meta_batch.rs`
(needs Studio UI). The instant-index engine (`index.rs`, CPE-703 / CPE-832-833) is explicitly **big-design,
confirm-attended**. Several research-library "next wave" plans (checkpoint-rollback, activity-replay,
agent-watch-dashboards, sidecar-cost, conflict-radar-close) are now **superseded** — their work shipped.

**3. QA burndown** (`MANUAL-TEST-BURNDOWN.md`): remaining rows are GUI (1/3/4/9), macOS (5), running-app binary
swap (6), or need containerized two-host CI infra (7, network E2E). Crucially, **CPE-1098 (cost ledger) and
CPE-1100 (radar) are fed by LIVE IPC only** (`ai-console://agent-cost` PTY-scraped usage; `ai-console://
fs-activity` from a live `notify` watcher racing two actors) — they have **no on-disk journal a fixture could
seed**, unlike the history tab (CPE-1130) and replay tab (CPE-1135) which read on-disk journals. So they are
genuinely NOT gui-smoke-pinnable the way those were — not merely "historically deferred".

## Apply
Before dispatching a fresh headless-work-hunt researcher next shift, read this first. If nothing has landed
that changes the above (check `git log` for new epics activated / GUI work done), the answer is still TAPPED —
skip the researcher and go straight to: surface the user-gated frontier to the user, or (if attended) get a
big-design go-ahead on the instant-index engine. See `[[headless-frontier-and-cpe-net]]`. Do NOT manufacture
filler modules.
