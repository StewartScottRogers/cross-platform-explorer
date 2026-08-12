# Doing — activated epics

Epics that have been **activated** (`/ticketing-epic activate CPE-NNN`): their open questions are
resolved and their child tickets exist in `Tickets/Backlog/`, each carrying `epic: CPE-NNN`.

- Frontmatter: `status: In Progress`, `tags: [epic]`.
- Unlike `Tickets/Doing/`, this folder is **not** one-at-a-time — several epics can be in flight.
- Closing an epic (`/ticketing-epic close CPE-NNN`) moves its file to [`../Done/`](../Done/wiki.md)
  and rewrites `status: Done` + `closed: YYYY-MM-DD`.

Folder location is the authoritative status — see [`../Backlog/wiki.md`](../Backlog/wiki.md) for the
whole map, and `Ticketing/wiki.md` → "Epics" for the lifecycle.
