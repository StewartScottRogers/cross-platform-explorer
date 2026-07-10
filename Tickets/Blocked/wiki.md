# Blocked

Tickets deferred because of an **external gate** that working cannot clear — e.g. a paid GitHub
plan (for Pages), code-signing certificates, a third-party or owner action, a date, an external
signoff.

A ticket here MUST have a Work Log note recording:
- **Blocked on** — exactly what is gating it.
- **Unblocks when** — the condition or action that clears it (prefer a "Next Actions — Owner"
  checklist so the unblock is turnkey).

Blocked is a side state, not a terminal one. Do NOT close blocked tickets as Won't Fix — they are
deferred, not declined. When the gate clears, move the ticket back to `Backlog/` (or straight to
`Doing/` via `/ticketing-work`). Blocked tickets are shown by `/ticketing-list` but are not offered
in its Work options.
