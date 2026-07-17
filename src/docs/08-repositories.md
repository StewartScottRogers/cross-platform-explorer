---
title: Repositories
order: 8
---

# Repositories

The Repositories view connects to a code forge, browses a repository in-app, and clones or syncs it to
disk. Open it from **Repositories** in the left sidebar.

## Browse and clone

Pick a **provider** (GitHub, GitLab, Bitbucket, Codeberg, or **Generic Git** for any URL), enter the
repository, and browse its tree. **Clone** downloads it into a folder you choose. A token (stored in your
OS keychain) unlocks private repositories.

## Generic Git and self-hosted

**Generic Git** clones or syncs **any** HTTPS/SSH remote, including self-hosted forges. Reaching a new
host requires your **explicit consent** — the app admits exactly that host (no wildcards), so self-hosted
works safely.

## Two-way sync

Once a repository is local, the explorer's status bar offers **Pull / Push / Sync…** with a preview of
the plan and a per-repo policy (merge / rebase / manual). It never force-pushes, and a divergence
surfaces for you to resolve — with an in-app conflict resolver when needed. Auto-sync on a schedule is
available and off by default.
