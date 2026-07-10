# Ticketing Organize — Done Folder

Scan the `Done/` folder and subdivide any directory that exceeds the file threshold
by moving its tickets one level deeper. Safe to run at any time — already-compliant
folders are untouched.

Then present an action menu following the rules in menu-render.md.

---

## Threshold

**50 .md files** per directory. A folder with <= 49 tickets is left as-is.

---

## Depth Levels

Each level is only created when its parent exceeds the threshold:

```
Level 0  Done/
Level 1  Done/YYYY/
Level 2  Done/YYYY/QN/
Level 3  Done/YYYY/QN/MonthName/
Level 4  Done/YYYY/QN/MonthName/Week-NN/    <- maximum depth
```

Quarter boundaries:
- Q1 = January, February, March
- Q2 = April, May, June
- Q3 = July, August, September
- Q4 = October, November, December

Week: ISO week number, zero-padded — `Week-03`, `Week-24`, `Week-52`.

---

## Algorithm

1. If $ARGUMENTS contains `dry-run`: report what would move without making any changes,
   then show the dry-run menu below.

2. Scan `Done/` recursively. Build a list of every directory that directly contains
   `.md` files (ignore directories that only contain subdirectories).

3. For each such directory where the direct `.md` file count >= 50:

   a. Determine the directory's current depth level (0-4).

   b. If already at Level 4: log `"[WARN] {path} has {N} files at maximum depth — manual review needed."` Skip it.

   c. For each `.md` file directly in the directory:
      - Read its `closed:` frontmatter field (format: `YYYY-MM-DD`).
      - If `closed:` is missing or blank: log `"[SKIP] {filename} — no closed date."` Leave it in place.
      - Compute the next-level subfolder name from the closed date.
      - Create the subfolder if it does not exist.
      - Move the file into the subfolder.

4. After processing all overfull directories, re-scan the newly created subfolders.
   If any of them now also contain >= 50 files, repeat step 3 for those folders.
   Continue until no directory exceeds the threshold (or max depth is reached).

5. Report a summary:
   ```
   Organized Done/:
     Moved  N files
     Created X new subfolders
     Skipped Y files (no closed date)
     Warnings Z (max depth reached)
   ```
   If nothing needed reorganising: `"Done/ is within threshold — no changes made (N files in largest folder)."`

---

## Safety Rules

- Never modify ticket file content — only move files.
- Never delete folders — leave empty parent folders in place.
- If a ticket's closed date puts it in a different year/quarter than its current parent folder,
  honour the closed date — move it to the correct location.

---

## Post-Run Menu

After completing, render an action menu following the rules in menu-render.md.

**After a real run** — HORIZONTAL:
```
┌─ Done Organised ─────┐
│  [1] Tasks           │
├──────────────────────┤
│  [2] Dismiss         │
└──────────────────────┘
```

**After a dry-run** — HORIZONTAL:
```
┌─ Dry-run Done ───────────────────┐
│  [1] Run  [2] Tasks              │
├──────────────────────────────────┤
│  [3] Dismiss                     │
└──────────────────────────────────┘
```

### [1] Tasks
Invoke /ticketing-list — shows the open queue with its action menu.

### [1] Run  *(dry-run only)*
Re-run this skill without the `dry-run` argument to perform the actual reorganisation.

### [2] / [3] Dismiss
Exit without action.
