---
id: CPE-1018
title: Fix reply_to_directive matching the wrong directive via substring (should be exact when-match)
type: bug
component: Backend
priority: medium
tags: ready
status: Doing
created: 2026-07-24
epic: CPE-810
estimate: 30m
---

## Summary
Found by the 2026-07-24 workshift bug-audit (fourth wave). `reply_to_directive`
(`crates/server/src/ticket_board.rs:356`) locates the target directive header with
`line.starts_with("### ▸ ") && line.contains(when)` — a **substring** test. If the requested `when` is a
substring of a *different* directive's header (its `when`, or the `to` target), the scan matches the wrong
header, so the reply is appended to — and `open→done` flipped on — the **wrong directive**, leaving the
intended one untouched.

Directive headers are `### ▸ {status} · to \`{to}\` · {when}` (see `append_directive` / `parse_directives`),
so `when` is the trailing ` · `-delimited field. Reachable from the agent-facing MCP path
(`ticket_mcp.rs` → `FsStore::reply` passes the client's `when` straight through; the only guard is
non-empty). The doc-comment says "the directive whose header carries `when`" — identity, not substring — so
this is a contract violation.

**Failing state:** a directives section with two entries whose `when` values are `"2026-...10:00:00Z"` and
`"...1..."`-style substrings of each other (or the simplified `when="10"` vs `when="1"`): resolving the
shorter `when` hits the longer header first (`contains` true) and mutates the wrong directive.

## Fix
Match the header's trailing `when` field **exactly** rather than by substring. In the scan loop, replace the
`line.contains(when)` test with an exact trailing-field comparison, e.g.:
```rust
if let Some(rest) = line.strip_prefix("### ▸ ") {
    // header body is "<status> · to `<to>` · <when>"; compare the when field exactly
    if rest.rsplit(" · ").next().map(str::trim) == Some(when) {
        header_start = Some(idx);
        break;
    }
}
```
(`rest.rsplit(" · ").next()` is the last field = `when` plus the trailing newline; `trim` drops it.) Keep the
rest of the rewrite logic unchanged.

## Acceptance Criteria
- [ ] With two directives whose `when` values are substrings of one another, `reply_to_directive(md, "<short>",
      ..)` resolves ONLY the directive whose `when` equals `"<short>"` exactly, leaving the other open/untouched.
      Add a regression test asserting this (it must fail against the current `contains` code).
- [ ] Existing `reply_to_directive`/`parse_directives` tests still pass; a normal exact `when` still resolves.
- [ ] `cargo test -p cpe-server ticket_board` green; clippy clean both feature modes; no new deps.

## Notes
Epic CPE-810 (the server/contract + agent-board surface owns ticket_board + the MCP). Backend-only, headless.
