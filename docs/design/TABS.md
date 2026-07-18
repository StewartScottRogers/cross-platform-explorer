# Tab bars — one standard

Every tab strip in the app (the main explorer window's `.tabbar`, the AI Console launcher's `#tabs`,
and any future one) uses the **same conventional active-tab treatment** so the selected tab is
unmistakable and the look is consistent everywhere.

## The rules

- **Active tab** is unmistakable:
  - an **accent bar along its top edge** — `box-shadow: inset 0 2px 0 0 var(--accent)` (the AI Console
    uses the system `AccentColor`-derived `var(--accent)`),
  - the **content surface** background (`--surface` / `Canvas`) so it lifts out of the bar and connects
    to the pane below (**no bottom border** — it merges with the content),
  - a **3-sided border** (top + left + right), full-strength text, `font-weight: 600`.
- **Inactive tabs** read as **distinct, recessed chips**, not just dimmer text: a subtle fill
  (`--surface-alt` / a faint grey) + a 1px inset border + dimmed text (`--text-dim` / `GrayText`).
- **Hover** on an inactive tab lightens its fill and restores full-strength text.
- Colours come from **theme variables** (`--accent`, `--surface`, `--surface-alt`, `--border`,
  `--text`, `--text-dim`) or system colours (`Canvas`, `CanvasText`, `GrayText`, `AccentColor`), so tabs
  are identical light/dark and cross-platform — never hard-code a colour.

## Where it lives

- Main window: `src/app.css` → `.tabbar` / `.tab` / `.tab.active` (used by `src/lib/components/TabBar.svelte`).
- AI Console: `sidecar/ai-console/src/launcher.html` → `#tabs` / `.tab` / `.tab.active`.

Keep the two in sync — a change to one is a change to both. New tab strips reuse these classes rather
than inventing their own active style.
