---
title: Sidecar Apps — Health & Recovery
order: 13
category: Agent Deck
categoryOrder: 3
---

# Sidecar Apps — Health & Recovery

The **Agent Deck**, **Agent Board**, and **Repositories** run as *out-of-process sidecars* — separate
programs the explorer launches and frames in their own windows, isolated from the plain file explorer. If
one ever misbehaves, everything you need to see why and fix it lives in one place.

## Settings → Platform

Open **Settings** and scroll to the **Platform** panel. Each sidecar shows:

- **A status pill** — at a glance:
  - **Running** (green) — up and serving.
  - **Ready** — installed and healthy, just not started yet.
  - **Missing** (red) — its program file isn't present (see self-healing below).
  - **Incompatible** — built against a newer contract than this app understands.
  - **Disabled** — you turned it off with the switch.
  - **Error** — it failed to start; the reason is shown on the row.
- **An enable/disable switch**, **version + contract** compatibility, a **health line** with the last error,
  and **View logs** for recent (secret-redacted) output.
- **Capabilities** — the permissions each sidecar requested (e.g. Secrets, Network). Grant or revoke each
  one right here; this is where consent lives (you're never interrupted at launch).

## The Repair button

Every sidecar row has a **Repair** button. It's the one-click fix for the common "stuck" states:

- **Reaps orphaned helper processes** that may be file-locking the program.
- **Drops a wedged connection** so the next launch starts clean.
- **Clears the stored error** and re-checks whether the program file is present.

It then tells you exactly what it did. If the program file is genuinely missing, Repair says so — and the
next section usually has already handled it for you.

## Self-healing (automatic)

You rarely need Repair, because the app heals itself on startup:

- **Auto-restore.** Each sidecar ships a pristine backup copy. If an update ever leaves a sidecar's program
  missing or stale (e.g. a file was locked during install), the app quietly restores it from that backup
  the next time it launches — no action needed.
- **Launch retry.** A momentary hiccup while starting a sidecar (a slow disk, an antivirus scan holding the
  file for a beat) is retried automatically before you'd ever see an error.

If a sidecar still won't start after all that, **Repair** first, and if it persists, reinstall the app —
the pristine backup is refreshed with every install.
