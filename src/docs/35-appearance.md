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
string values used to sit at 3.7:1 — visibly dim against the preview pane — and now measure 5.0:1
or better. Buttons and focus rings are unchanged.

## What's next

Theme and Contrast are the first slices of the broader appearance program. Native accent colour and
window materials are still to come — this page will grow to cover them as they ship. Nothing about how
the Theme or Contrast controls work today will change underneath them.
