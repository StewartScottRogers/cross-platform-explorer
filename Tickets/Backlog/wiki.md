# Backlog

Open tickets waiting to be worked. Each is a `CPE-NNN_slug.md` file whose frontmatter
`status:` is `Open`.

## How to file
- Interactively: `/ticketing-new` (recommended — assigns the next ID and estimate).
- By hand: copy `../_template.md`, name it `CPE-NNN_short-kebab-title.md`, fill the frontmatter.

## Priority guide
- **Critical** — app crashes, data loss, updater/release pipeline broken.
- **High** — a core feature is broken; workaround painful or absent.
- **Medium** — feature works but behaves incorrectly.
- **Low** — cosmetic, minor inconvenience, nice-to-have.

## Working a ticket
`/ticketing-work CPE-NNN` — moves it to `../Doing/` and takes it through to close.
`/ticketing-list` shows the queue with an action menu (work all / subset / one / resequence).
