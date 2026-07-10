# Skills Organise

Manage Claude Code skills as named feature sets. A feature set is a group of related
skills sharing a common kebab-case prefix (e.g. `ticketing-*`). Skills with no shared
prefix are classified as General.

Pass a command as $ARGUMENTS: `list`, `new`, or `reorganise [prefix]`.
If no argument is given, run `list`.

After each command, present an action menu following the rules in menu-render.md.

Note: in this project, skills are plain markdown files in `.claude/commands/` tracked by git.
There is no `.projitems` / Visual Studio registration to maintain — creating or renaming a file
IS the whole operation.

---

## list

Show all skills in `.claude/commands/` grouped by feature set.

1. Glob `.claude/commands/*.md`
2. Group by shared prefix: if two or more skills share the same text before the first `-`,
   they form a feature set. Skills with a unique name (or no `-`) go under **General**.
3. Output:

```
Feature Sets:
  ticketing-*  (5):  ticketing-list, ticketing-new, ticketing-work, ticketing-organize, ticketing-setup
  menu-*       (1):  menu-render
  skills-*     (1):  skills-organise

General (N):
  <any single-prefix skills>
```

Then render the post-list menu:

```
┌─ Skill Sets ───────────────────────────┐
│  [1] New feature set  [2] Reorganise   │
├────────────────────────────────────────┤
│  [3] Dismiss                           │
└────────────────────────────────────────┘
```

### [1] New feature set
Run the `new` command below.

### [2] Reorganise
Ask: "Which prefix?" then run the `reorganise [prefix]` command below.

### [3] Dismiss
Exit without action.

---

## new

Scaffold a new feature set interactively.

1. Ask (one message):
     Prefix:      (kebab-case, e.g. "deployment")
     Description: (one sentence — what this feature set does)
     Skills:      (comma-separated suffixes, e.g. "run, rollback, status")

2. For each skill suffix, ask for a one-line description of what it does.

3. Create `.claude/commands/{prefix}-{suffix}.md` for each, with this stub:
   ```markdown
   # {Prefix} {Suffix}

   {Description}

   ## Steps

   *(Implement this skill)*

   ## Menu Extension Point

   Follows menu-render.md rules.
   ```

4. Report: "Created {N} skills in the {prefix}-* feature set: {list}."

Then render the post-new menu:

```
┌─ Next Step ──────────┐
│  [1] List            │
├──────────────────────┤
│  [2] Dismiss         │
└──────────────────────┘
```

---

## reorganise [prefix]

Move existing skills into a named feature set, renaming files and updating all references.

1. Run `list` to show the current skill inventory.

2. Ask: "Which skills should join the `{prefix}-*` feature set?" (comma-separated, current names without `.md`)

3. For each selected skill:

   a. Determine the new filename:
      - If the skill already has a different prefix (e.g. `old-name`), strip the old prefix and use the remainder as the suffix: `{prefix}-{remainder}.md`
      - If the skill has no prefix (e.g. `build`), use the full name as the suffix: `{prefix}-build.md`
      - If the skill already has the correct prefix, skip it.

   b. Read the old skill file.

   c. Write the new file at `.claude/commands/{new-name}.md` with the same content.

   d. Find and update all references to the old skill name in:
      - All `.claude/commands/*.md` files (other skills that reference it)
      - `CLAUDE.md`
      - `Tickets/wiki.md` and all `Tickets/**/*.md`
      - Any `.md` files in the project root
      Replace `/old-name` -> `/new-name` and `old-name.md` -> `new-name.md`.

   e. Delete the old file.

4. Report a full change summary:
   ```
   Reorganised into {prefix}-*:
     Renamed: old-name -> {prefix}-suffix  (references updated in N files)
     …
   ```

Then render the post-reorganise menu:

```
┌─ Next Step ──────────┐
│  [1] List            │
├──────────────────────┤
│  [2] Dismiss         │
└──────────────────────┘
```

---

## Conventions

- Prefix: lowercase kebab-case, 1-3 words, describes the feature domain
- Suffix: describes the action the skill performs (`list`, `new`, `work`, `run`, `setup`, `organise`)
- A skill belongs to a feature set when its filename starts with `{prefix}-` and at least one other skill shares that prefix
- Singletons (only one skill with a given prefix) are classified as General until a second skill joins
