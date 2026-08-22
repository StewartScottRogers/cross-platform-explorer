---
id: CPE-1850
title: the signing step round-trips tauri.conf.json through a lossy JSON reserialiser
type: task
priority: Low
status: Backlog
tags: ready
estimate: M
created: 2026-08-21
closed:
---

## Problem

The Windows code-signing step reads `src-tauri/tauri.conf.json`, parses it with `ConvertFrom-Json`,
adds the `bundle.windows` signing config, and writes the whole file back with
`ConvertTo-Json -Depth 40`.

That is a **general JSON round trip through a reserialiser** on the release path. CPE-1842 measured it
as **semantically inert against today's manifest** on both PowerShell hosts — top-level key order
preserved, values identical, only whitespace and (on 5.1) 22 harmless `\u0027`/`\u003e` escapes differ.
So there is no correctness defect today.

It is not inert against a manifest that later gains:

- a **JSONC comment** — Tauri's own config loader accepts them; `ConvertFrom-Json` does not preserve them
- a **large integer** that does not survive the parse/serialise round trip intact
- a **duplicate key**, where reserialising silently picks one
- **deep nesting past 40**, where `-Depth 40` truncates rather than failing

None of those exist in the file right now. All of them are ordinary things a config file acquires.

## Why Low, and why a ticket at all

Nothing is broken and the failure needs a future edit to trigger. But the hazard currently has **no
record anywhere** — CPE-1842 scoped it out correctly (it was an S-sized encoding fix; this is a design
change) and the reasoning would otherwise be lost.

The signing step is also the least-watched code in the repo, operating on one of the five files that
must stay version-synchronised.

## The shape of the fix

Stop rewriting `tauri.conf.json` at all. Move the `bundle.windows` signing patch into **`tauri-action`'s
`--config` overlay**, so the signing configuration is supplied alongside the manifest rather than spliced
into it. The file then stays byte-identical to what is committed, and the whole class — encoding, key
order, escapes, depth, comments — stops applying.

## Acceptance criteria

- [ ] The signing step no longer reads-modifies-writes `src-tauri/tauri.conf.json`, or the decision to
      keep doing so is recorded with what makes it safe.
- [ ] If the overlay route is taken: verify the produced installer is equivalent to one built the old
      way. CPE-1842's UAT could establish that the reformatted manifest parses to an identical object
      graph through Tauri's exact load path, but explicitly **could not** establish that the resulting
      installer is byte-identical — no real bundle was built. That gap has to close here.
- [ ] Both `release.yml` and `release-sidecar.yml`, not one. Every partial sweep in this repo has been
      caught by a reviewer.
- [ ] CPE-1842's guard (`src/lib/workflowPwshFileEncoding.test.ts`) is updated or retired coherently — if
      the step stops touching the file, the guard's subject changes.

## Notes

Filed from the CPE-1842 review, where the independent Reviewer agreed the scope-out was correct but asked
that the hazard be recorded rather than left implicit.

One thing that ticket established and this one should not re-derive: the two halves of the old encoding
bug failed in **opposite directions** — the BOM half fail-loud (broke the build at config parse), the
mojibake half fail-silent (parsed clean and shipped). They always fired together, so the loud one masked
the silent one. Any change here must keep that property in mind: a partial change to this step can be
worse than none.

Related: CPE-1842 (the encoding fix, merged), CPE-1834 (the same codec fix in `scripts/release.ps1`),
CPE-1841 (that script's version regex).
