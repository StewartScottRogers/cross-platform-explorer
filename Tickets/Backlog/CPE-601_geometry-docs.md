---
id: CPE-601
title: "Document the launch geometry options (README + in-app docs)"
type: Task
status: Open
priority: Low
component: Docs
tags: [ready]
epic: CPE-580
estimate: 30m
created: 2026-07-17
---

## Summary
Document the CLI window-geometry flags so they're discoverable and the contract is unambiguous.

## Acceptance Criteria
- [ ] README gains a "Launch options" section: every flag (`--x --y --width --height --position
      --monitor --maximized --fullscreen --physical`), the **precedence** (`CLI > saved state >
      default`), the **logical-pixel** contract (+ `--physical`), and the off-screen-clamp behaviour.
- [ ] An in-app **Documents** library entry covers the same (per [[maintain-in-app-docs-library]]); if it
      earns a section, add its section→doc registry entry ([[CPE-595]]) too.
- [ ] Examples match actual behaviour (`cpe --x 100 --y 100 --width 1200 --height 800`).

## Notes
Do this once behaviour is settled ([[CPE-598]]–[[CPE-600]]).
