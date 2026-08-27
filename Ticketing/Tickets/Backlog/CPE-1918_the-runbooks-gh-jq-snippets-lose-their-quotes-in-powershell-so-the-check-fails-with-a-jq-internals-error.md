---
id: CPE-1918
title: the runbooks' `gh --jq` snippets lose their embedded quotes in PowerShell, so a release check fails with a jq internals error instead of its crafted message
type: bug
priority: Low
status: Open
tags: ready
estimate: XS
created: 2026-08-27
---

## Summary

Two runbook snippets tell a human to copy-paste a `gh ... --jq '...select(.name=="x")...'` command
into PowerShell. On Windows PowerShell 5.1 that does not work: PowerShell strips the embedded `"`
characters when marshalling a single-quoted string to a native executable's argv, so `jq` receives
the selector **unquoted** and tries to parse the hyphenated identifier as chained function calls.

Observed verbatim by an independent UAT tester on 2026-08-27, running the `RELEASING.md` snippet
against real run `32645968281`:

    gh : function not defined: sidecar/0

Root cause confirmed with `node -e "console.log(JSON.stringify(process.argv.slice(1)))"`: `jq` is
handed `.jobs[] | select(.name==verify-published-manifest-sidecar)` — bare and unquoted.

## Where

- `RELEASING.md` — the new sidecar publish check (`verify-published-manifest-sidecar`), added by
  CPE-1908.
- `.claude/commands/run.md` line ~55 — the equivalent plain-channel check. **This is the original;
  CPE-1908 copied an existing broken pattern rather than inventing a new one.**

The correct shape already exists two lines above the broken one in `run.md`:
`--jq ".[] | select(...==\"<TAG>\")"` (double-quoted outer, escaped inner).

## Why this is Low and not High

**It fails safe.** `$job` ends up falsy either way, so the following `throw "...do not publish"`
still fires and still blocks the human from publishing. Nothing unsafe gets through. The cost is
purely diagnostic: at 2am the operator sees a confusing jq internals error instead of the crafted
"this job did not pass, do not publish" message, and may waste time debugging `gh` rather than
reading the actual release state.

## Acceptance criteria

- [ ] Fix the quoting in **both** `RELEASING.md` and `.claude/commands/run.md`, using the
      already-correct escaped-double-quote shape that sits two lines away in `run.md`.
- [ ] Actually run each fixed snippet, verbatim, in Windows PowerShell 5.1 against a real run id,
      and paste the output in the Work Log. A runbook fix that is only reasoned about is the same
      class of defect as the bug.
- [ ] Sweep for the same pattern elsewhere — `grep` for `--jq '` across `*.md` and `.claude/` — and
      fix every instance, rather than the two we happen to have tripped over.
- [ ] Consider whether a guard can pin this at all (e.g. a test that extracts fenced PowerShell
      snippets from the runbooks and asserts they contain no single-quoted `--jq` with embedded
      double quotes). If that is not worth the machinery, say so and why.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1039's independent UAT finding. Not a regression in
CPE-1908 — the tester explicitly confirmed the pattern pre-dates it.

Separate observation from the same UAT run, recorded here so it is not lost: `verify-release-artifacts`
prints "…AND that pubkey/endpoints match the second in-repo pin…" **unconditionally**, including on
runs where `--skip-pin-check` bypassed exactly that check. That is a success message claiming a
verification that did not happen. It belongs with **CPE-1901** (`--skip-pin-check` is a one-token kill
switch for the updater pin), not here.
