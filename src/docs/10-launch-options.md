---
title: Launch options
order: 10
---

# Launch options — window geometry

You can position and size the window from the command line. Every flag is independently optional — set
any subset and the rest keep their defaults.

## Examples

```
cpe --x 100 --y 100 --width 1200 --height 800   a 1200×800 window at (100, 100)
cpe --position center --width 1000              centred, 1000 wide, default height
cpe --monitor 1 --maximized                     maximized on the second display
```

## Flags

- **`--x` / `--y`** — window position (left / top edge).
- **`--width` / `--height`** — window size.
- **`--position <preset>`** — `top-left`, `top-right`, `bottom-left`, `bottom-right`, or `center`. An
  explicit `--x` / `--y` overrides the preset for that axis.
- **`--monitor <n>`** — target display (0-based); positions/centres relative to that monitor.
- **`--maximized` / `--fullscreen`** — start maximized or fullscreen.
- **`--physical`** — treat the sizes/positions as physical pixels instead of logical.

Run `cpe --help` to see the full list.

## The rules (so nothing surprises you)

- **Units are logical pixels** by default — the same visual size regardless of the display's scaling.
  `--physical` switches to physical pixels.
- **Precedence:** a command-line flag beats your saved window state, which beats the built-in default.
  Omit a flag and the saved/default value stands.
- **It can't strand the window.** An off-screen or too-large request is clamped onto the monitor's
  visible area (with a note on the console), so the window always opens where you can see and grab it.
  Non-numeric, zero, or negative values stop the launch with a clear error rather than a broken window.
