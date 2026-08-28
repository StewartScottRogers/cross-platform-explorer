---
title: Appearance
order: 35
category: Appearance & Input
categoryOrder: 9
---

# Appearance

Settings has an **Appearance** section with a **Theme** control — this is where the app's colour theme
lives.

## The Theme control

Open **Settings → Appearance** and you'll find a **Theme** dropdown with three options:

- **System** — follow the operating system's light/dark setting.
- **Light** — always use the light theme, regardless of the OS.
- **Dark** — always use the dark theme, regardless of the OS.

**System** tracks your OS preference live: if you flip your OS between light and dark mode while the
app is open, the app follows along immediately — no restart needed. Choosing **Light** or **Dark**
pins the app to that theme explicitly, unaffected by whatever the OS is set to.

Choosing any of the three is safe and instant — there's no separate "Save" step, the choice applies
immediately and persists across restarts, the same as every other row on the Settings page.

## The Contrast control

Right below Theme, a **Contrast** dropdown offers three options, independent of your Light/Dark/System
choice:

- **Off** — always use the normal colour palette.
- **System** — read the operating system's high-contrast accessibility state once at startup and match
  it (Windows high contrast, macOS "Increase contrast", or the Linux desktop's high-contrast setting).
  It's a one-shot check at launch, not live tracking yet — flipping the OS setting while the app is
  running won't take effect until the next launch; if the OS signal can't be read, System behaves like Off.
- **High** — always switch to the AAA high-contrast palette, regardless of the OS.

Like Theme, Contrast applies immediately and persists across restarts — no separate "Save" step, and
it composes with whichever Theme you've chosen (e.g. High contrast with Dark theme).

## Accent-coloured text stays readable

Some text is tinted with the app's accent blue rather than the ordinary text colour — JSON string
values in the structured preview, links inside a Markdown or notebook preview, the rename dialog's
"new name" column, the status bar's hidden-files count, and the short accent notes in several
dialogs. That blue is now picked **separately from the accent used for buttons, focus rings and
icons**, because those two jobs pull in opposite directions: a blue dark enough for white button
text to sit on is too dark to read as small text on a dark background.

The practical effect: accent-tinted text meets the WCAG AA contrast bar (4.5:1 for normal text) on
every background it's painted on, in every Theme and Contrast combination. In the Dark theme, JSON
string values used to sit at 3.2:1 against the preview pane — visibly dim — and now measure 5.0:1
or better. Buttons and focus rings are unchanged.

## Log-viewer WARN rows in High contrast

The log viewer tints WARN rows and badges amber. Until now the High contrast palettes never chose
their own amber, so both of them silently borrowed the one calibrated for the ordinary Light theme's
white background. On High contrast + Dark that put the WARN badge at 3.3:1 against the pane it is
painted on — below the 4.5:1 readable-text bar, in the very setting chosen for legibility. Both High
contrast palettes now use their own amber (13.0:1 on High contrast + Dark, 7.8:1 on High contrast +
Light). The Light and Dark themes are unchanged.

## The AI Console follows your system, not the Theme control

The AI Console (the Agent Deck window) is a separate window with its own stylesheet, and it does **not**
read the Theme or Contrast settings above. It follows your **operating system's** light/dark preference
directly. So switching the explorer to Dark while your OS is set to Light leaves the Agent Deck light —
that is expected, not a bug.

Its colours are now measured rather than assumed. Several were readable in one scheme and not the
other, and one was hard to see in both:

- **Keyboard focus is now easy to find.** The Agent Deck draws no focus outline; a coloured border on
  the field itself is the only indicator you get. Against a dark input's interior that border measured
  2.5:1 — under the 3:1 bar for a visual indicator — so tabbing through the window was genuinely hard
  to follow. It now uses a lighter blue in dark mode and clears the bar.
- **The grid view's per-pane headers were black on near-black in light mode.** The agent name, state
  and cost strip above each terminal tile inherited your system's text colour while painting a fixed
  dark background, so in light mode it read at 1.1:1 — effectively invisible. Those headers now set
  their own light text.
- **Inactive session tabs, the ✕ close buttons, the "Close all" hover, the installed/not-installed
  badges and the "Starting the Agent Deck…" message** all sat under the 4.5:1 readable-text bar in at
  least one scheme, several of them only while hovered or mid-animation. All now clear it.
- **The context-size caption beside each model in the model menu** ("200k ctx") was dimmed to the
  point of sitting two hundredths above the bar. It is still visibly secondary — the smaller type does
  that job — but it is no longer borderline.

Two things are knowingly still out, and both are tracked separately because both are shared with the
main explorer window rather than local to the Agent Deck:

- The small coloured **chip carrying each session's number** picks from a palette the explorer's
  Agents list uses too, and two of its colours are too light for the white numeral on them.
- The small **status dot** on each session tab and pane header (amber "blocked", blue "working",
  green "done") uses two colours that are too weak to carry meaning by colour alone. They are not
  the only signal — hovering a dot names the state, and the grid view spells it out in words beside
  it — but they will be re-tuned with the chip palette.

## What's next

Theme and Contrast are the first slices of the broader appearance program. Native accent colour and
window materials are still to come — this page will grow to cover them as they ship. Nothing about how
the Theme or Contrast controls work today will change underneath them.
