---
id: CPE-601
title: "Document the launch geometry options (README + in-app docs)"
type: Task
status: Done
priority: Low
component: Docs
tags: [ready]
epic: CPE-580
estimate: 30m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Document the CLI window-geometry flags so they're discoverable and the contract is unambiguous.

## Acceptance Criteria
- [x] README gains a "Launch options" section: every flag (`--x --y --width --height --position
      --monitor --maximized --fullscreen --physical`), the **precedence** (`CLI > saved state >
      default`), the **logical-pixel** contract (+ `--physical`), and the off-screen-clamp behaviour.
- [x] An in-app **Documents** library entry covers the same (per [[maintain-in-app-docs-library]]); if it
      earns a section, add its section→doc registry entry ([[CPE-595]]) too.
- [x] Examples match actual behaviour (`cpe --x 100 --y 100 --width 1200 --height 800`).

## Notes
Do this once behaviour is settled ([[CPE-598]]–[[CPE-600]]).

## Resolution
README gains a "Launch options" section (flags table, precedence, logical-px contract, clamp behaviour,
examples). New in-app doc `src/docs/10-launch-options.md` covers the same. Not a UI section, so no
section→doc registry entry needed.
