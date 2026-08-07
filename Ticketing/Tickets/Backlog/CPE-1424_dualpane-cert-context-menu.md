---
id: CPE-1424
title: "Dual-pane right-pane cert/JWT management (pane-aware context menu)"
type: Feature
status: Backlog
priority: High
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-07
---
## Scope
Add crypto management to the context menu, PANE-AWARE (route via `ctx.inPaneB`, exactly like CPE-1377/1384), so
it works from the RIGHT pane in commander mode:
- Right-click a cert/CSR file (`.pem/.crt/.cer/.der/.csr`) → "Sign with CA…" / "Issue cert from this CSR…" (opens
  the CPE-1423 Sign/Issue dialog pre-filled with the clicked file as the CSR or CA), and "Inspect" (ensures the
  preview shows the decode — already auto-decodes on select, so this just focuses/selects it).
- Right-click a `.jwt` → "Inspect JWT" (select → preview decode).
- Right-click empty space / a folder → "Create certificate here…" (opens the CPE-1423 Create dialog with the
  clicked folder as the default output location).
Gate each entry on the file type (only show cert/CSR ops on cert/CSR files, etc.). Route the action to the
active/clicked pane's folder + selection via `paneStateFor(inPaneB)` (the established pattern), so a pane-B menu
creates/writes into pane B's folder. Add tests (pane-B routing, like App.paneBBulkOps — the clicked file/folder +
pane are honored). Docs. Depends on CPE-1423.
