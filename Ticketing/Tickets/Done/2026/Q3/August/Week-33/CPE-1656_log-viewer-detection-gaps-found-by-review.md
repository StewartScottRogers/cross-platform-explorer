---
id: CPE-1656
title: Log viewer — u16-index-table binaries can read as UTF-16, and Go/Ruby/Rust traces no longer group
type: bug
priority: Low
status: Backlog
tags: ready
estimate: M
created: 2026-08-11
closed:
---

## Problem

Two measured gaps from the independent re-review of PR #842. Both are narrow, neither regresses anything
a user has today, and both were deliberately kept out of that PR so its blocking fix could land.

### A. A small-uint16 index table can be mistaken for UTF-16 text

CPE-1644 replaced `classify_nul_bytes`'s "the minority byte lane must contain **exactly zero** NULs" rule
with a ratio (`UTF16_MINORITY_NUL_RATIO = 0.25`), because the strict-zero rule rejected any UTF-16 log
containing a single non-ASCII character. That fix is right, and the reviewer confirmed it against real
`notepad.exe` (45.7% minority), ELF (40.6%), SQLite (92.2%), WAV (92.2%), BMP (7.0%, majority never
clears its bar), TIFF, ICO and protobuf — all still correctly `Binary`, with wide margins.

But a **sequential small-`u16` little-endian table** — the shape of font glyph/loca tables and resource or
index directories — measures ~0.4% on the minority lane and ~99.6% on the majority, so it now classifies
as `Utf16Le` and would render as garbage. The old strict-zero rule happened to reject it.

The deeper point, worth recording: 0.4% sits **below** the ~1-2% minority-lane noise floor measured for
genuine UTF-16 text, so **no threshold on this heuristic alone can separate the two.** Distinguishing them
needs a different signal (decoded-codepoint plausibility, a printable-ratio check on the decoded text, or a
structural check), not a tuned constant. The in-code comment claiming a "wide margin... well below the
binary floor" currently oversells the confidence and should be softened to match.

No real file format tested triggers this — only a synthetic-but-plausible shape.

### B. Go, Ruby and Rust stack traces no longer group under the Errors filter

CPE-1638's fix for over-grouping made bare indentation necessary-but-not-sufficient: a continuation now has
to match a known frame shape (`at `, Python `File "..."`, `Caused by:`, `... N more`). That correctly
stopped unrelated indented output from inheriting an error's level, and Java/.NET/Node/Python traces still
group fully. But three formats that used to benefit from indentation alone now group nothing:

| Format | Frames grouped |
|---|---|
| Go (`goroutine …`, `main.main()`, tab + `/app/main.go:10`) | 0 of 3 |
| Ruby (`\tfrom /path:N:in \`method'`) | 0 of 2 |
| Rust backtrace (`   0: symbol` + indented `at /path:N`) | 0 of 4 |

Rust is the worst of the three: the non-conforming `0: symbol` line breaks the chain, so the *conforming*
`at …` line immediately after it is never even tested.

This is consistent with the module's stated "prefer under-grouping to over-grouping" bias, so it is a
completeness gap rather than a correctness bug — but Rust traces matter here, since this app is largely Rust.

## Acceptance criteria

- [ ] A small-`u16` index table no longer decodes as UTF-16 text — via a signal other than a tuned NUL
      ratio (decoded-codepoint plausibility / printable-ratio / structural check). Prove it with the
      reviewer's fixture AND re-prove every format they cleared (PE, ELF, SQLite, WAV, BMP, TIFF, ICO,
      protobuf) still classifies correctly, with the ratios recorded.
- [ ] Genuine UTF-16 logs containing non-ASCII (emoji, accented Latin, CJK, RTL) still decode — the
      CPE-1644 regression must not come back. Re-run against the real Windows UTF-16 logs the UAT used
      (Microsoft Edge update log, MSI install logs).
- [ ] The in-code comment on `UTF16_MINORITY_NUL_RATIO` states the real limitation instead of claiming a
      wide margin.
- [ ] Go, Ruby and Rust frame shapes group under the Errors filter, including the Rust case where a
      non-conforming line sits between the header and the conforming frames (chain-breaking must not skip
      recoverable frames).
- [ ] The over-grouping guard CPE-1638 added still holds: an indented but unrelated line must NOT inherit a
      preceding error's level. Re-run that test — widening the shapes must not undo it.
- [ ] `npm run check` + full vitest green; `cargo clippy --all-targets -D warnings` clean in all three CI
      feature combos; crates/server suite green.

## Notes

- Source: independent re-review of PR #842, 2026-08-11 — measured, with a ratio table and a format table.
- Related: [[CPE-1644]] UTF-16 logs, [[CPE-1638]] stack traces survive filtering, [[CPE-1636]] prose false
  positives, [[CPE-1655]] errors with no level word are invisible.
- Sequence with CPE-1655 — both widen the same detector and should be designed together.

## Work Log

- 2026-08-11 — Filed by the Foreman from the PR #842 re-review findings.
