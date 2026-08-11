---
id: CPE-1605
title: "Smart folder sidebar tooltip mislabels itself \"this saved search\" in all 12 locales"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Found writing the CPE-1604 docs split (epic CPE-1569) while verifying the sidebar's Smart Folders and
Saved Searches sections against the actual UI strings — the app ships **two** distinct virtual-folder
features side by side (Smart Folders = one saved tag query; Saved Searches = a structured multi-field
query), and this string tells the user they're looking at the wrong one.

## The bug
`src/lib/components/Sidebar.svelte:567` sets a smart-folder row's tooltip from the `smart.itemTip` i18n
key:

```svelte
title={$t("smart.itemTip", { tag: sf.tag })}
```

but the string itself describes a **saved search**, not a smart folder:

```
"smart.itemTip": "Files tagged “{tag}” — click to open this saved search",
```

(`src/lib/i18n.ts:286`). Hovering any Smart Folders row tells the user "click to open this saved search" —
wrong feature name, and actively confusing now that both "Smart Folders" and "Saved Searches" are separate,
adjacent sidebar sections with genuinely different capabilities (single tag vs. multi-condition query).

This isn't a one-locale typo: the same wrong wording — "saved search" (or its translation) instead of
"smart folder" — is baked into the `smart.itemTip` key in **all 12 shipped locales** (grep
`"smart.itemTip"` in `src/lib/i18n.ts`: en, es, de, fr, it, pt, nl, pl, ru, zh, ja, ko all say some
variant of "click to open this saved search").

## Reproduction
1. Tag a file, right-click the tag in the sidebar's Tags section → **Save as smart folder**.
2. Hover the new row under **Smart Folders** in the sidebar.
3. Tooltip reads "Files tagged "…" — click to open this saved search" — should say "smart folder".

## Fix
Correct the `smart.itemTip` string (and its translation in the other 11 locale blocks) to say "smart
folder" instead of "saved search" — mirroring how `smart.searchItemTip` (the row tooltip actually used for
Saved Searches rows, `Sidebar.svelte` saved-search `{#each}` block) correctly says "Saved search — click to
open, right-click to rename/delete."

## Acceptance criteria
- `smart.itemTip` in every locale in `src/lib/i18n.ts` refers to a smart folder, not a saved search.
- A quick manual hover check (or a guard test asserting the key text doesn't contain "saved search") confirms
  the fix.

## Notes
Conflict surface: `src/lib/i18n.ts` only (the `smart.itemTip` key across all locale blocks). No component
change needed. Model: haiku — purely a string fix, no logic.
