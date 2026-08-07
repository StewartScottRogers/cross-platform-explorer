---
id: CPE-1423
title: "Frontend: certificate management dialogs (Create cert, Sign/Issue from CSR)"
type: Feature
status: Backlog
priority: High
component: Frontend
tags: [ready]
epic: CPE-1417
created: 2026-08-07
---
## Scope
Two dialogs wiring the shipped backend commands:
- **Create Certificate** (`commands.certCreate(params, certPath, keyPath)`): form = common name, SAN DNS names +
  IPs (reflowing pill inputs), validity days, key type (EC-P256/P384, RSA-2048/4096 — EC default), is_ca toggle;
  a native path-picker (Browse) for the output folder / cert+key filenames (path inputs need a picker — memory).
  On submit, writes cert.pem + key.pem to the chosen location; success toast; the new files appear in the listing.
- **Sign / Issue from CSR** (`commands.certIssueFromCsr(csrPath, caCertPath, caKeyPath, validityDays, outCertPath)`):
  pickers for the CSR, the CA cert, the CA key, validity days, and the output cert path; on submit writes the
  issued cert.
Follow the app dialog conventions (visible border, MENUS/TABS standards, light theme, path-picker on every path
field). Add jsdom render-specs (mock the commands; assert the right command + args on submit, validation gating,
cancel). Wire an entry to open each (command palette + the context menu in CPE-1424). Docs page + sectionDocs
entry (CPE-579). Depends on CPE-1420/1421 (shipped).
