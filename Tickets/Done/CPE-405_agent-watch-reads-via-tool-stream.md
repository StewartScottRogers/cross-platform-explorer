---
id: CPE-405
title: Agent Watch — surface the agent's file READS (via its tool-output stream)
type: feature
priority: medium
estimate: L
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [big-design, agent-watch]
epic: AGENT-WATCH.md
depends-on: CPE-398
---

## Problem / value
AGENT-WATCH.md wants reads/writes/edits/deletes. The CPE-398 filesystem watcher covers mutations
but CANNOT see reads (a Windows FS watcher doesn't report opens/reads). Reads are the missing half
of "understand what the agent is doing" — knowing which files it consulted explains its edits.

## The only viable source (design note)
Reads can't come from the filesystem — they must come from the agent's OWN activity. The AI Console
already runs the agent in a PTY and captures its output (the session ring, CPE-385). For agents
that announce tool calls (Claude Code prints Read/Edit/Bash tool invocations), parse the stream for
read operations and emit them alongside the FS watcher's mutations.

## Scope / risks
- Per-agent parser (start with Claude Code's output format); agent-agnostic fallback = none.
- Feed parsed reads into the same `ai-console://fs-activity` channel with a new `read` kind, so the
  timeline (CPE-400) + row annotations (CPE-399) show them (a distinct, dimmer style).
- Fragile by nature (depends on the agent's output format) — must degrade silently when the format
  isn't recognized; never block or corrupt the terminal stream.

## Acceptance
- [x] Claude Code reads appear in the timeline + as (distinct) row annotations while watching
- [x] Unknown/other agents: no reads, no errors (graceful)
- [x] Parser is unit-tested against captured sample output; terminal I/O unaffected

## Work Log
2026-07-14 — Picked up. Estimate: L (unchanged). Confirmed dependency **CPE-398 is Done**. Mapped
the rail: the FS watcher + `ai-console://fs-activity` emit live in the **host** (`src-tauri`), while
the agent's PTY output is captured in the **sidecar** (`console.rs` reader thread). Sessions already
cross sidecar→host→frontend via `Event::Status{state:"session:…"}` → `app.emit("ai-console://session")`
(CPE-396). Reads will ride that exact rail with a new `fs-read:` prefix.

2026-07-14 — Built the parser core (AC3): `sidecar/ai-console/src/agent_reads.rs` — a stateful,
dependency-free `ReadScanner` that line-buffers the output stream (joining a tool-call line split
across chunks), strips ANSI (CSI + OSC), and matches Claude Code's `● Read(<path>)` tool-call
shape while rejecting prose, non-Read tools, result lines (`⎿ Read 42 lines`), and truncated paths.
Bounded so a newline-less runaway line can't grow memory. **8 unit tests** incl. a realistic
multi-line captured snippet with ANSI and an unknown-agent graceful case.

2026-07-14 — Wired the sidecar tap: the `console.rs` reader thread feeds each output chunk through
the scanner (read-only — it never alters the stream to the ring/terminal) and, per detected read,
announces `read_announcement(cwd, raw)` = `fs-read:<json {path}>` with the path resolved to absolute
against the session's Project folder (so it matches the watcher's absolute paths). Extracted
`read_announcement` as a testable seam (+1 test).

2026-07-14 — Host: added a match arm forwarding `fs-read:<json>` Status frames onto the SAME
`ai-console://fs-activity` channel with `kind:"read"` (malformed payloads ignored). Frontend: added
`read` to the `FsActivity` kind + `normalizeFsActivity` allow-set; made `read` the **weakest** fold
precedence (never downgrades a mutation; a mutation upgrades a read) and excluded it from
`affectsListing` (a read never re-lists); added the `read` label + a **distinct dimmer/hollow** badge
& row accent in `AgentTimeline.svelte` and `FileList.svelte`. **+4 vitest** tests for read
ingestion/precedence/no-relist/graceful-unknown-kind.

2026-07-14 — Verified green end-to-end at every seam: `cargo test -p ai-console` **141 passed**;
`cargo clippy -p ai-console --all-targets` clean; `cargo clippy -p <host> --features sidecar-platform`
compiles clean (host arm); `npm run check` **0 errors / 0 warnings**; `npm test` **390 passed** (incl.
14 in agentActivity.test.ts). Terminal I/O is unaffected — the tap only *reads* a copy of each chunk.

## Resolution

Delivered the full vertical: the agent's file **reads** — which a Windows FS watcher structurally
cannot see — now flow from the agent's own tool-output stream into Agent Watch's timeline and row
annotations, styled distinctly (dimmer/hollow) as a consult rather than a change.

**Files changed:**
- `sidecar/ai-console/src/agent_reads.rs` *(new)* — the `ReadScanner` parser + 8 tests (AC3).
- `sidecar/ai-console/src/lib.rs` — export the module.
- `sidecar/ai-console/src/console.rs` — reader-thread tap + `read_announcement` seam (+1 test).
- `src-tauri/src/lib.rs` — host arm re-emitting `fs-read:` onto `ai-console://fs-activity` as `kind:"read"`.
- `src/lib/sidecar.ts` — `read` kind on `FsActivity` + `normalizeFsActivity`.
- `src/lib/agentActivity.ts` — weakest-precedence fold + `affectsListing` exclusion.
- `src/lib/components/AgentTimeline.svelte`, `FileList.svelte` — `read` label + distinct dimmer badge/accent.
- `src/lib/agentActivity.test.ts` — +4 read-kind tests.

**Tradeoffs / by design:** the parser is Claude-Code-specific and fragile by nature (the ticket says
so) — it degrades **silently** for other agents (no reads, no errors) and can miss reads if Claude's
output format changes; that's acceptable for a best-effort visibility signal and never affects the
terminal. Chunk→text uses `from_utf8_lossy`, so a multibyte path char split exactly on an 8 KiB read
boundary could mangle rarely; paths are overwhelmingly ASCII. Adding other agents' formats is a
future extension (new match cases in the scanner), not required here.
