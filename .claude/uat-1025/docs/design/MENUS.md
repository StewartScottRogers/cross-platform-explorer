# Menu design standard (CPE-491)

The single source of truth for **every popup menu** in the app — right-click context menus and
click-open dropdowns alike. New menus follow this; existing ones are brought into line. The goal is
that a user can't tell which component drew a menu: they all look and behave identically.

Applies to: `ContextMenu` (file right-click, the canonical reference), `AgentMenu` (Agents leaf /
Agent Deck button), `TabMenu` (tab right-click), `MenuBar` dropdowns, `CommandBar` sort/view/filter
menus, and the Agent Deck's own menus in `sidecar/ai-console/src/launcher.html`.

> Not covered: the **CLI** action menus (the ASCII boxes in `.claude/commands/*`), which have their
> own spec in `.claude/commands/menu-render.md`. Different medium, different rules.

## How menus are built (two shared implementations)

There are two, both conforming to everything below — reuse them, don't invent a third:

1. **Dropdowns** (open below a trigger) use the **global** classes in `src/app.css`:
   `.menu` (container) · `.menu button` (item) · `.menu-sep` (separator) · `.menu .check` (a ✓ in
   `var(--accent)` marking the active value). Used by `CommandBar` (sort/view/filter) and `MenuBar`.
   New dropdowns should use these classes — no local copy.
2. **Context menus** (open at the cursor) use the `.ctx` + `.row` pattern, currently defined
   per-component (`ContextMenu`, `AgentMenu`, `TabMenu`, `PreviewPane`'s `.text-ctx`). They are
   consistent with each other and with the table below. *(Follow-up: extract this into shared global
   classes like `.menu`, so the copies collapse to one — tracked, not required for compliance.)*

**Active/selected value:** a menu that tracks a current choice (sort key, view mode, filter, locale)
marks it — a leading `✓` in `var(--accent)` (the `.check` element), or `background: var(--selection)`
on the row. Do this consistently.

**Audit (2026-07-16):** every menu conforms after removing the `AgentMenu` red. Dropdowns via the
global `.menu`; context menus via `.ctx`; the console's `launcher.html` menus via system colors.

---

## Cross-platform first

- **Custom-rendered, not native.** Menus are our own DOM, not the OS menu — so they are pixel- and
  behaviour-identical on Windows, macOS, and Linux. We do **not** try to mimic each OS's native menu.
- **Colour comes only from theme variables**, never a hard-coded hex. The tokens below resolve to the
  right light/dark values on every OS (and follow the app's theme toggle). A literal like `#d05656`
  is a bug — it ignores the theme and diverges per platform. (This was exactly the `AgentMenu` red
  that prompted this standard.)
- **The console webview is separate.** `launcher.html` can't see `app.css`, so it can't use these
  variables — but it follows the *same rules* using CSS **system colors** (`Canvas`, `CanvasText`,
  `ButtonText`, `Field`) which are themselves theme-/OS-aware, plus the same structure below.

---

## Container

| Property | Value |
|----------|-------|
| position | `fixed`, `z-index: 100`, placed at the cursor and **clamped into the viewport** (never clipped off-screen) |
| background | `var(--surface)` |
| border | `1px solid var(--border-strong)` (a **visible** edge, not just a shadow — see the dialogs rule) |
| radius | `var(--radius-lg)` |
| shadow | `0 10px 30px rgba(0,0,0,0.16)` |
| padding | `5px` |
| min-width | `190px` compact · `210px` for a rich/file menu |
| focus | `outline: none` on the container; it takes focus so Escape/arrows work |

## Items

Each item is a `<button class="row" role="menuitem">`.

| Property | Value |
|----------|-------|
| layout | `display:flex; align-items:center; gap:10px; height:32px; padding:0 10px; text-align:left; border-radius:var(--radius)` |
| **text colour** | inherited **`var(--text)`** — the same for *every* item, including destructive ones. **Never** set a per-item colour. |
| hover | the global `button:not(:disabled):hover { background: var(--hover) }` (app.css) — do **not** re-declare a bespoke `.row:hover` |
| active/selected | `background: var(--selection)` when a menu tracks a current value (e.g. the sort menu's checked row) |
| disabled | `opacity: 0.5`, not clickable |
| icon | leading `<Icon>` at `size={15}`, optional but consistent within a menu |
| shortcut hint | trailing `<span class="hint">` — `margin-left:auto; color:var(--text-faint); font-size:12px` |

## Separators

`<div class="sep" role="separator" />` — `height:1px; background:var(--border); margin:4px 6px`.

## Destructive actions

**Do not colour destructive item text red.** In this app the file `ContextMenu` renders *Delete* in
the normal `--text`, so red menu text reads as foreign. Convey destructiveness by **wording** ("Delete",
"Close", "Close all…") and, for anything **irreversible**, a follow-up **`ConfirmDialog`** (whose
*primary button* is the only place red belongs — `danger`). Menu text stays `--text`.

## Behaviour

- **Escape** closes; a **click or right-click outside** closes.
- Opens **at the cursor**, clamped to the viewport.
- The container is focused on open so keyboard users land in it.
- Labels are **i18n** via `$t(...)` (never a bare string) — same as the rest of the chrome.

---

## Checklist for a new (or audited) menu

- [ ] Container matches the table above (tokens, border, clamp-to-viewport).
- [ ] Items are `.row` buttons; text is `var(--text)`; hover is the global button hover; no bespoke colours.
- [ ] Destructive items are worded, not coloured; irreversible ones route through `ConfirmDialog`.
- [ ] Escape + click-outside close it; it opens at the cursor and stays on-screen.
- [ ] Labels go through `$t`.
- [ ] (Console menus) same rules via CSS system colors, since `app.css` isn't available there.
