# Blocked — externally gated epics

Epics parked because of an **external gate** that working cannot clear — hardware we do not have, a
paid plan, a certificate, a third-party or owner action, a date. Same meaning as
`Tickets/Blocked/`, applied to the epic queue.

- Frontmatter: `status: Blocked`, `tags: [epic]`.
- An epic here MUST record **Blocked on** (exactly what gates it) and **Unblocks when** (the
  condition that clears it) in its Notes.
- Blocked is a side state, not a terminal one: when the gate clears, move the file back to
  [`../Backlog/`](../Backlog/wiki.md) (`status: Proposed`) or [`../Doing/`](../Doing/wiki.md)
  (`status: In Progress`). Never close a blocked epic as Won't Fix.

**Blocked vs Deferred:** use `Blocked/` only for a gate *outside* our control. If *we* chose to
postpone it — doable, just waiting on an internal prerequisite or deprioritized — it belongs in
[`../Deferred/`](../Deferred/wiki.md). If in doubt: can working alone unblock it? Yes → Deferred.
No → Blocked.

This folder is normally empty; it exists so the Epics queue is genuinely the same shape as
`Tickets/` and an epic can be parked without inventing a folder later (CPE-1676). This `wiki.md`
is also what keeps the folder in git — git does not track empty directories.
