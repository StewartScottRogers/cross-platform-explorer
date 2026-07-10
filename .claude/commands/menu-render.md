# Menu Render — Shared Menu System

Defines how to render action menus consistently across all cross-platform-explorer skills.
Any skill that presents results and offers follow-up actions should follow these rules.
This is a living spec — new items, groups, and rules are added here as the system grows.

---

## Core Principle

Menus appear AFTER output, never before. They extend what the user just saw rather than
interrupting it. The menu is a prompt for action, not a navigation tree.

---

## Layout Decision

Evaluate the menu items before rendering. Choose the layout as follows:

```
Total items <= 5  AND  all labels <= 12 chars  AND  no descriptions needed
    -> HORIZONTAL

Total items > 5  OR   any label > 12 chars   OR   descriptions are needed
    -> VERTICAL

Multiple logical groups exist
    -> GROUPED (horizontal within each group, groups stacked vertically)

Groups where one item needs a short description
    -> HYBRID
```

---

## Bounding Box

Every menu is wrapped in a Unicode box. Use these drawing characters:

```
┌─┐   corners:    ┌ (U+250C)  ┐ (U+2510)  └ (U+2514)  ┘ (U+2518)
│ │   vertical:   │ (U+2502)
└─┘   horizontal: ─ (U+2500)
├─┤   separator:  ├ (U+251C)  ┤ (U+2524)   replaces the plain --- line
```

**Width rule:** find the longest content line, add 2 (one space padding each side).
That is the inner width. The `─` count in top / bottom / separator rows = inner width.
All shorter lines are right-padded with spaces to fill the box.

---

## Box Header (Title Bar)

Every menu box has a title embedded in its top border. The title names the context —
it answers "what is this box?" before the user reads the options.

**Format:**
```
┌─ Title ─────────────────────────────┐
```

**Construction rule:**
- Start: `┌─ ` (3 chars)
- Title: 2-4 words, title case (e.g. `Ticket Actions`, `Build Failed`, `Next Step`)
- After title: ` ` then enough `─` to reach the box width, then `┐`
- Formula: dashes after title = inner_width - 3 - len(Title) - 1

**Naming convention:**

| Situation | Title pattern | Examples |
|-----------|--------------|---------|
| Actions on a resource | `{Resource} Actions` | `Ticket Actions`, `Dashboard Actions` |
| Result of an operation | `{Resource} {Outcome}` | `Build Passed`, `Tests Failed`, `Done Organised` |
| Post-create / post-complete | `Next Step` | `Next Step` |
| After a preview/dry-run | `{Operation} Done` | `Dry-run Done` |
| Skill inventory view | noun describing what's shown | `Skill Sets` |

Context-sensitive titles are encouraged — `Build Passed` vs `Build Failed` adds signal
without requiring the user to read the content first.

---

## Formats (all with title bar)

### HORIZONTAL
```
┌─ Title ──────────────────────────────┐
│  [1] Action  [2] Action  [3] Action  │
├──────────────────────────────────────┤
│  [N] Dismiss                         │
└──────────────────────────────────────┘
```

### VERTICAL
```
┌─ Title ────────────────────────────────────────────────────┐
│  1  Long action label     short description of what it does │
│  2  Another action        another description               │
├────────────────────────────────────────────────────────────┤
│  3  Dismiss                                                 │
└────────────────────────────────────────────────────────────┘
```

### GROUPED
```
┌─ Title ──────────────────────────────┐
│  Group A:  [1] Item  [2] Item        │
│  Group B:  [3] Item                  │
├──────────────────────────────────────┤
│            [N] Dismiss               │
└──────────────────────────────────────┘
```

### HYBRID
```
┌─ Title ──────────────────────────────────────────────┐
│  Group A:  [1] Item  [2] Item                        │
│  Group B:  [3] Item — description of this one        │
├──────────────────────────────────────────────────────┤
│            [N] Dismiss                               │
└──────────────────────────────────────────────────────┘
```

---

## Rules

- **Title bar**: always present. Short (2-4 words), title case, describes context not actions.
  Context-sensitive: change the title when the situation changes (passed vs failed, etc.).
- **Separator** (`├──┤`): always before exit/dismiss options. Length matches inner width.
- **Grouping**: items operating on the same target go on the same line.
  Dismiss/Cancel/Quit always go below the separator.
- **Context sensitivity**: omit items that do not apply — never show disabled items.
- **Labels**: <= 12 chars, title case, no punctuation.
- **Descriptions**: one clause, <= 50 chars, lowercase. VERTICAL or HYBRID only.
- **Numbers**: sequential from 1, always gapless after omissions.
- **Response**: execute the chosen action immediately. Handle follow-up questions inline.

---

## Adding a New Menu Item

1. Decide which group it belongs to (or create a new group).
2. Give it a label (<= 12 chars) and optional description (<= 50 chars).
3. Add it to the skill's menu definition in the appropriate position.
4. Re-evaluate the layout rule — adding an item may change the format.
5. Update the skill's title if the new item changes the context.
6. Document the behaviour in the skill file.
7. Add a row to the Changelog below.

---

## Changelog

| Date | Skill | Change |
|------|-------|--------|
| 2026-07-10 | ticketing-list | Initial menu: Work (All/Subset/One), View (Resequence), Dismiss |
| 2026-07-10 | ticketing-new | Post-create menu: Work it now / File another / Dismiss |
| 2026-07-10 | ticketing-work | Post-close menu: Next ticket / Tasks / Dismiss (queue-aware) |
| 2026-07-10 | ticketing-organize | Post-run menu: context-sensitive real/dry-run variants |
| 2026-07-10 | skills-organise | Post-list/new/reorganise menus |
