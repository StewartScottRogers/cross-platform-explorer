# Done — closed epics

Epics whose children are all Done and whose Definition of Done holds. Closed by
`/ticketing-epic close CPE-NNN`, which rewrites `status: Done` + `closed: YYYY-MM-DD` and moves the
file here.

- Frontmatter: `status: Done`, `tags: [epic]`.

## Flat, deliberately — no dated nesting (CPE-1676)

`Tickets/Done/` is nested as `Done/YYYY/QN/Month/Week-NN/` because it holds **thousands** of
tickets and `/ticketing-organize` has to keep it browsable. This folder is **flat and stays flat**:
there are ~70 epics in total and few ever close, so dated buckets would add depth without buying
anything, and every reader (both boards, the MCP, the guard test) gets a simpler contract.

`/ticketing-organize` therefore archives `Tickets/Done/` **only** and must not touch
`Epics/Done/`. Revisit only if this folder ever passes a few hundred files.

## Historically closed epics live elsewhere too

Epics closed before CPE-1676 were filed into `Tickets/Done/` (including its dated subfolders) and
were left there — moving them would rewrite years of archive paths for no gain. Both board
implementations therefore read epics from `Ticketing/Epics/**` **and** from `Tickets/Done/`, and
will keep doing so.
