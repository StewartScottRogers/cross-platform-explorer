---
title: Agent Deck
order: 4
category: Agent Workspace
categoryOrder: 10
---

# Agent Deck

The Agent Deck runs **coding agents** (Claude Code, aider, Codex CLI, Gemini CLI, and others — 12
bundled) against a folder, using the provider and model you choose. It's an **out-of-process sidecar** in
its own window, not a panel in the main explorer — so agents keep running even if you close it. Open it
from the **Agent Deck** button on the Application toolbar. That button (and **Repositories**, next to it)
only appears on a **sidecar-platform** build; on a plain build there's nothing to click, and Agent Board
is the only out-of-process app you'll see instead.

## Launch an agent

The launcher is one form:

| Field | What it does |
|---|---|
| **Agent** | The coding tool to run — 12 ship today (Claude Code, Aider, Codebuff, OpenAI Codex CLI, Gemini CLI, Grok CLI, Mistral Vibe, opencode, Pi Coding Agent, Qwen Code, Tau, VTCode). An install badge next to the form shows whether it's installed; click **Install** if not. |
| **Provider** | Who serves the model — the tool's own built-in login (needs no key) or a gateway like OpenRouter, Together, Groq, and others, depending on what the agent supports. |
| **Account** | Only appears once a provider has **two or more** labelled keys saved — picks which one this launch uses. |
| **Model** | Type to filter, pick from the dropdown, or leave it **blank** for the agent's default model. The one exception: for a local **LM Studio** provider, blank stays blank so the agent picks up whatever model LM Studio currently has loaded, rather than a fixed default. |
| **Project folder** | The folder the agent works in, typed or picked with **Browse…**. |

Click **Launch**. Each launch opens its own **tab**, backed by its own terminal (with a real PTY) —
the sidecar runs every session concurrently, so launching a second or third agent doesn't wait on the
first. A running agent also appears under **Agents** in the explorer's left sidebar the moment it
launches.

## Provider and model

A provider without a saved key still needs no key at all if it's the tool's own built-in login (e.g.
`claude login`) — those show up in the Provider list alongside the API-key gateways. For a gateway
provider, the **Model** field is a real combobox: it lists the gateway's models from a periodically
refreshed, verified snapshot (fast, works offline) and falls back to a live fetch when the snapshot
doesn't cover that gateway or you force a refresh. Type to filter by id or name — a matched model shows
its context length and per-token price when known.

## Keys…

Click **Keys…** to add a provider API key. It's stored in your **OS keychain** — never in a file, and
the panel never displays a saved value again once you close it. The dialog is: pick a **Provider**, an
optional **label** (e.g. "work" vs "personal" — this is what fills the Account picker once you have two
or more for the same provider), paste the key, then **Check** to verify it against the provider before
**Save** commits it. A saved key can be **removed**, but not renamed — relabeling one means deleting it
and saving it again under the new label.

## Save setup

Behind **Advanced ▾** sits **Saved setup**: a named **provider + model** combination you can reload
later from the **Setup** dropdown, scoped **per agent** — a setup saved under Claude Code doesn't show up
when Aider is selected. It deliberately **does not save your key** (that's what Keys… is for). A setup
can be **overwritten** (save again under the same name) or **deleted** (the **✕** next to the dropdown),
but not renamed in place. The same Advanced row also has **Fast model** (a cheaper/faster model for minor
steps, optional) and **API key (this launch)** — a one-off key used just for this launch, without saving
it anywhere.

## Keep agents current — Manage agents ▾

- **Check for updates now** — fetches new/updated agent definitions (signed).
- **Auto-update on open** — does that automatically each time you open the Agent Deck.
- **Pin this agent** — freezes the currently selected agent's version, skipping updates for it.
- **Reset to the shipped agents** — undoes any updates, back to what shipped with the app.
- **Roll back a version…** — steps an agent back to a previous signed version.

**What the result message means.** "Check for updates now" spells out which situation you're
actually in, rather than reporting every outcome as the same reassuring line:

- *Updated N agents* — new definitions were published, verified, and installed. The agent list
  re-renders with them straight away.
- *You already have the latest published agents* — the published catalog offers exactly the
  versions you're running. Routine and healthy; nothing to do. This is what an ordinary check
  returns most of the time, between one release and the next.
- *Agents are already up to date* — same good news, from a check that found nothing listed to
  reconsider at all.
- *The published agent catalog has gone backwards* — the catalog is offering **older** versions
  than the ones you already have, so they weren't installed. The message says how many entries are
  affected ("1 of the 4 published agent entries…"), so a single bad entry doesn't read as a
  wholesale failure. Updates are only ever accepted going forwards, which is exactly why nothing
  was installed here.
- *The published catalog looks corrupted or mis-signed* — the catalog listing verified, but the
  individual agent definitions it named didn't, so nothing was installed. Again, your existing
  agents are untouched and safe to use.
- *The published catalog was refused* — the catalog **listing** itself was rejected, before anything
  it names was acted on. The listing is downloaded to a temporary folder first, so that much did
  happen; what did not is anything that follows from trusting it — no agent definitions were
  fetched, nothing was written to your agent folder, and the temporary copy is discarded. That
  covers a listing whose signature doesn't check out, one that is corrupt or unreadable, one written
  for a newer version of the app, and one naming an agent under a name the app won't use as a
  filename. The app deliberately doesn't guess between them in the message, because all four are
  publishing-side faults with the same answer for you: nothing changed, and there is nothing to do.
- *Couldn't check / you're offline* — the check never completed. Nothing changed; try again later.
- *Your record of installed agent versions is unreadable* — the app keeps a small local file noting
  which version of each agent you have. It is how updates are only ever accepted going forwards. If
  that file is damaged, the app cannot tell an upgrade from a downgrade, so it **refuses to install
  anything** rather than guessing — a guess here would mean happily reinstalling very old agents.
  (The download itself already happened; it is the *applying* that stops.) This is the one message
  in this list that points at your machine rather than the publishing side. Nothing was changed or
  installed, and your agents keep working. **Reset to the shipped agents** clears the usual case,
  after which the next check installs the current published versions; if the message persists, that
  file has to be deleted by hand. **Roll back a version…** reports the same thing for the same
  reason — it needs the same record to know what it is rolling back from.

**Otherwise, if you see the amber bar there is nothing to fix on your machine.** Every one of the
other states leaves your installed agents exactly as they were, and they keep working. The problem
is on the publishing side, and the message clears itself the next time a good, newer catalog is
published — so the useful response is to carry on and check again later, not to reset or roll
anything back.

## Recent…

Click **Recent…** for a list of past sessions, kept across restarts. **Relaunch** reopens one with the
same agent, provider, model, and folder it originally ran with; or open its transcript, which is
**secret-redacted** before it's shown.

## Sessions and the sidebar

A launched session runs in a separate, host-owned process from the Agent Deck window itself — that's
what lets it **survive closing and reopening the console**: closing the window (or the whole console UI)
doesn't stop what's running, and reopening the Agent Deck reattaches to every session already in flight.
Every running session also shows under **Agents** in the explorer's left sidebar, whether or not the
Agent Deck window is even open, each with a small colour chip shared with its tab (and with Agent Watch,
if you're watching that folder — see [Agent Watch](explorer-agent-watch)).

**Double-click** an Agents leaf to open (or focus, if it's already open) the Agent Deck window scoped to
that session's tab. If the Agent Deck is already open, double-clicking just focuses the existing window
and shows a notice ("Agent Deck is already open — click the agent's tab to focus it.") rather than
re-scoping it. **Right-click** an Agents leaf, or the Agent Deck toolbar button itself, for a small menu:

- **Open ⟨agent · provider · model⟩** — jump to that session's tab (leaf only).
- **Close ⟨agent · provider · model⟩** — end just that one session; the rest keep running (leaf only).
- **Close all consoles** — after a confirm ("Every running agent will be terminated…"), genuinely
  terminates **every** running agent, wherever its session lives, and clears the sidebar's Agents list —
  matching the Agent Deck window's **own** "Close all" button exactly (see *Limits / notes* below).

## "Work on this" — a scoped launch from the explorer

Right-click a file, a selection of files, or a folder in the main explorer pane for **Work on this in
Agent Deck** (or **Work on this folder in Agent Deck** for a single folder). It opens the Agent Deck
window with the **Project folder** and a task hint already filled in — the folder you right-clicked, or
the folder containing your file selection, plus a hint naming what you selected — but it doesn't
auto-launch; you still pick Agent/Provider/Model and click Launch yourself.

## Multiple tabs, and Grid

Every launch gets its own tab; tabs can't be renamed or reordered, and closing the **last** tab doesn't
close the window — the toolbar just collapses to an empty state, ready for the next launch. With more
than one session open, switch **Tabs ⇄ Grid** to tile every agent side by side instead of one at a time —
see the [Agent Grid](05-agent-grid) page for how tiling, focus, and throttling work.

## Run a swarm

**Run swarm ▾** starts a small **team** on one task: a **coordinator** that plans and dispatches, plus
**one builder per task line** you write (not a single fixed builder — three task lines means three
builders). They coordinate through a shared **mailbox** (post/read messages to hand off work and report
progress) and shared **memory** (write/recall notes, linked with `[[wiki links]]`) so context carries
across agents instead of being re-derived by each one — both are visible live in the swarm panel while it
runs. Type one task per line (optionally `glob :: task` to scope a line to certain files, letting
disjoint tasks run in parallel), click **Start**, and each team member opens in its own tab like a normal
launch. It's labelled **experimental** in the app itself. New to it? **Load a demo ▾** fills in one of
twelve ready-made, narrated missions (grouped Simple / Complex / Messaging / Shared-memory) that only
create files — nothing is deleted — so you can watch a swarm run end to end before writing your own
tasks. See the [Swarms](09-swarms) page for the full walkthrough.

## Permissions, and Repair

The console runs as its own **sandboxed process** and only gets the permissions you grant it — the
**Secrets** permission is what lets it store your API keys in the keychain; skip it and Keys… simply
won't persist anything (you're asked again next time). Review or revoke permissions from the app's
sidecar Settings.

If the Agent Deck can't start at all, you get an error toast — "Agent Deck couldn't start — open Settings
→ Platform to see why and Repair it." — rather than a silent no-op. **Settings → Platform** lists every
sidecar with a health pill (**Missing** / **Incompatible** / **Ready** / **Healthy**, worst-first) and its
own **Repair** button there (not inside the Agent Deck window). Repair reaps any orphaned background
processes left over from a bad prior start and re-checks the binary, reporting either **"Repaired: ⟨what
it did⟩"** or **"Repair failed — the platform may be off"** when the binary is genuinely missing and
can't self-heal.

## Worked example

You want an agent to add tests to one module while you keep browsing the rest of the project.

1. Right-click the module's folder in the explorer and choose **Work on this folder in Agent Deck** — the
   Agent Deck opens with that folder already set.
2. Pick **Claude Code** as the Agent, a Provider with a saved key (or a built-in login), leave Model
   blank for the default, and click **Launch**.
3. Keep working in the main explorer window — the agent keeps running in its own tab regardless.
4. Check on it later from the sidebar's **Agents** entry (double-click to jump to its tab), or open
   **Agent Watch** on that folder to see its file changes as they happen.

## Limits / notes

- **"Close all consoles" (sidebar / toolbar) terminates every running agent, then clears the sidebar's
  Agents list.** Sessions run in a separate, host-owned process that's designed to survive the console UI
  closing (that's exactly what makes reattach-on-reopen work); "Close all consoles" reaches into that
  process the same way the Agent Deck window's **own** "Close all" button (inside the console, which
  explicitly warns "Any running agents will be terminated") does — both routes end at the same
  termination, so either one leaves nothing still running (**CPE-1621**). A confirm dialog stands between
  the click and the termination for exactly this reason: this is a batched, irreversible stop of
  everything at once. Ending a **single** session (the leaf's own "Close ⟨agent⟩" item) is unconfirmed
  and unaffected by any of this — it always just ends that one session, as before.
- **A setup, a key label, and a tab name are three different things that can't be renamed in place** —
  each can only be deleted/removed and recreated under a new name.
- **Swarms share the Agent Deck's trust model.** A swarm agent only gets the working folder and
  credentials you granted the launch — there's no separate, wider permission surface for a swarm than a
  single launch has.
- **The Agent Deck keeps one small folder in your system temp directory** — `cpe-ai-console`. It holds
  the session-daemon's diagnostic log (and, on the reattach path, the note of which port that daemon is
  listening on), so a restarted console can find work that's still running. It's deliberately at a fixed
  name, because being findable by name is the whole point of it.
  Since the name is fixed, it's also predictable, so the app now **refuses to use it if it isn't a plain
  folder**. If something has replaced `cpe-ai-console` with a shortcut, junction or symbolic link
  pointing somewhere else, the Agent Deck stops writing there rather than following it — agents keep
  running, only the diagnostic log goes quiet. The app never deletes what it finds there: if you hit
  this, look at that entry yourself and remove it by hand (**CPE-1975**).
