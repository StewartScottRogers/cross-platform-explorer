# Done

Closed tickets — never deleted. Terminal statuses: `Done`, `Won't Fix`, `Duplicate`.

A closed ticket contains a filled-in **Resolution** section (what changed, which files, why, any
tradeoffs) and `closed: YYYY-MM-DD` in its frontmatter.

## Folder depth
Tickets start directly in `Done/`. When a directory exceeds 50 files, `/ticketing-organize`
subdivides it one level deeper by closed date:

```
Done/ -> Done/YYYY/ -> Done/YYYY/QN/ -> Done/YYYY/QN/MonthName/ -> Done/YYYY/QN/MonthName/Week-NN/
```

## Reopening
Move the file back to `Backlog/`, set `status: Open`, and add a Work Log note explaining why.
