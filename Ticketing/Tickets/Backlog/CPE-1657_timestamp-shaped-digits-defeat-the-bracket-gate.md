---
id: CPE-1657
title: Any digit-separator-digit run passes as a "timestamp", letting bracket-tagged prose colour as a level
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-11
closed:
---

## Problem

CPE-1636 (PR #842, round 3) stopped bracket-tagged prose from being read as a log level by requiring a
**timestamp** somewhere in the lead-in before honouring a `[...]` tag — real Logback lines always carry
one, prose never does. That closed the four reported cases.

The verifying reviewer then attacked the new gate with five adversarial inputs and **defeated it twice**:

```
At 14:30 the [main] ERROR handling was disabled.   -> null    (held)
Version 1.2-30 [beta] ERROR counts rose.           -> null    (held)
See section 3-14 [note] WARNING signs.             -> null    (held)
2026-08 [draft] ERROR budget                       -> "error" (FALSE POSITIVE)
10.0.0.1:8080 [proxy] ERROR rate                   -> "error" (FALSE POSITIVE)
```

Cause: `TIMESTAMP_SHAPE_REGEX = /\d{1,4}[:-]\d{2}/` matches **any** digit-separator-digit run, not a
timestamp. A date fragment (`2026-08`) or an IP:port octet (`.1:80`) satisfies it exactly as well as a
clock time.

Worth noting *why* the three that held, held: each contained an extra prose word outside the bracket
("the", "Version", "See section") that tripped the older isolated-word fallback — **not** because the gate
recognised the digits as bogus. So the gate is doing less work than it appears to; the fallback is
carrying it. Both defeating inputs need the bracket to be the only letters in the lead-in *plus* a
coincidental digit run.

## Why this was not blocked at merge

It is strictly narrower than the class it replaced (which caught plain English sentences and markdown
checklists — this repo's own tickets are full of those), both defeating inputs are log-fragment-shaped
rather than natural sentences, and an independent scan of all 3,859 non-blank lines of `src/docs/*.md`
produced **zero** instances. Same tier as the already-accepted CPE-1655/1656 gaps and the documented
RFC3164 non-fix.

## Acceptance criteria

- [ ] The two defeating inputs above classify as `null`, and stay covered by regression tests.
- [ ] The timestamp check recognises an actual time/timestamp shape rather than any digit-separator-digit
      run — decide whether that means anchoring it positionally, requiring a full `HH:MM:SS`/ISO-8601
      shape, or dropping the gate for something structural, and record the reasoning.
- [ ] The positive controls still detect: `17:04:22.123 [main] ERROR c.e.MyService - Failed to connect`,
      `2026-08-11T09:14:05Z [1234] ERROR worker crashed`, and `[ERROR] msg` (level inside the bracket — a
      separate code path that must not become collateral damage).
- [ ] The four original prose cases (`[main]`, `[TODO]`, `[1]`, `[x]`/`[ ]`) stay `null`.
- [ ] The `src/docs/*.md` corpus false-positive count stays at 1 or lower (the remaining one is CPE-1655's
      `## Error handling` markdown heading).
- [ ] Re-run the 12-format classification table; no real format may regress.

## Notes

- Source: independent verification of PR #842 round 3, 2026-08-11 — the reviewer's own adversarial inputs.
- Related: [[CPE-1636]] prose false positives, [[CPE-1655]] errors with no level word,
  [[CPE-1656]] detector gaps found by review.
- Sequence with CPE-1655 and CPE-1656 — all three widen or tighten the same detector and should be
  designed as one pass rather than three separate tweaks.

## Work Log

- 2026-08-11 — Filed by the Foreman at merge time; the reviewer approved PR #842 with this recorded as a
  fast-follow rather than a blocker.
