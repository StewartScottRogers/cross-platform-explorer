<script lang="ts">
  // Embedded terminal dock panel (CPE-1243, epic CPE-714): an xterm.js pane per tab, backed by a real PTY
  // (CPE-1242) via `terminalClient.ts`'s framework-agnostic wiring, with tabs via the `terminal_tabs` dock
  // model (`terminal_dock_*`, CPE-947). Mounted only while the panel is toggled on (App.svelte's
  // `{#if showTerminal}`), so an unused panel costs nothing — no PTY, no dock tab, no xterm instance — and
  // `onDestroy` closes every session it opened, so toggling the panel off never leaks a shell (CPE-1242's
  // "no live entry left behind" DoD, extended to the whole panel).
  //
  // xterm usage (Terminal + FitAddon + open/onData/loadAddon) mirrors the AI Console launcher
  // (`sidecar/ai-console/src/launcher.html`'s `buildTerm`/`refit`) — the proven in-repo reference for
  // wiring xterm.js to a live PTY stream.
  import { onMount, onDestroy, tick, createEventDispatcher } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import Icon from "./Icon.svelte";
  import {
    PtyBridge,
    openTerminalTab,
    closeTerminalTab,
    followNavigation,
    shellChoicesFor,
    basename,
    type ShellChoice,
  } from "../terminalClient";

  /** The current folder (App.svelte's `currentPath`); `""` means no real folder (Home/places/archive) —
   *  a new tab then opens at the backend's own default cwd instead of a bogus path. */
  export let cwd = "";

  const dispatch = createEventDispatcher<{ close: void }>();

  interface Tab {
    id: number; // terminal_dock tab id
    cwd: string;
    term: Terminal;
    fit: FitAddon;
    bridge: PtyBridge;
  }

  let tabs: Tab[] = [];
  let activeId: number | null = null;
  let opening = false;
  /** Set when the most recent `addTab` failed (bad shell, spawn error, ...) — cleared on the next attempt.
   *  Surfaced inline rather than silently swallowed, per [[avoid-modal-permission-popups]] (plain text, no
   *  modal). */
  let openError = "";

  /** Follow navigation (CPE-1243): when on, navigating the explorer `cd`s the ACTIVE tab's live shell to
   *  match (see `terminalClient.ts::followNavigation`'s doc comment for why a `cd`, not a respawn). Off by
   *  default — a terminal the user is mid-command in shouldn't have folders typed into it unasked. */
  let followNav = false;

  const platform = typeof navigator !== "undefined" ? navigator.platform || navigator.userAgent : "";
  const shellChoices: ShellChoice[] = shellChoicesFor(platform);
  /** The shell for the NEXT new tab — a running tab's shell can't change after it's spawned. */
  let selectedShell: string | null = null;

  $: activeTab = tabs.find((t) => t.id === activeId) ?? null;
  $: effectiveCwd = cwd && cwd.trim() ? cwd : null;

  function makeTerm(): { term: Terminal; fit: FitAddon } {
    const term = new Terminal({
      fontFamily: "ui-monospace, Consolas, 'Cascadia Mono', monospace",
      fontSize: 13,
      cursorBlink: true,
      scrollback: 5000,
      // Renders xterm's accessibility tree (`.xterm-accessibility-tree`, one real-text-content row per
      // terminal line) alongside the visible canvas — a genuine screen-reader win for a live shell, and
      // also the stable DOM signal gui-smoke's terminal-panel.smoke.ts asserts against (xterm otherwise
      // paints only to canvas, which headless WebDriver can't read text out of).
      screenReaderMode: true,
      // xterm's `theme` option needs literal colours (it paints to canvas, not CSS) — these mirror the
      // app's single light theme (`app.css` `:root`: --surface #fff, --text #1b1b1b, --accent #0067c0,
      // --selection #e5f0fb). The app has no dark theme yet (see CLAUDE.md); revisit if one lands.
      theme: {
        background: "#ffffff",
        foreground: "#1b1b1b",
        cursor: "#0067c0",
        selectionBackground: "#e5f0fb",
      },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    return { term, fit };
  }

  function refit(tab: Tab): void {
    try {
      tab.fit.fit();
    } catch {
      // The container can be zero-sized for a moment during mount/switch — fit() no-ops safely; the next
      // resize (tab switch, ResizeObserver tick) retries.
    }
    void tab.bridge.resize(tab.term.rows, tab.term.cols);
  }

  /** Svelte action: attach the tab's already-created `Terminal` to its host div once it exists in the DOM,
   *  and size it to the real container (xterm buffers writes fine before `open()`, so PTY streaming can
   *  start — and does, in `addTab` — before this ever runs). */
  function mountXterm(el: HTMLDivElement, tab: Tab) {
    tab.term.open(el);
    refit(tab);
  }

  async function addTab(atCwd: string | null): Promise<void> {
    if (opening) return;
    opening = true;
    openError = "";
    const { term, fit } = makeTerm();
    try {
      const { dockId, bridge } = await openTerminalTab(atCwd ?? "", selectedShell, term);
      const tab: Tab = { id: dockId, cwd: atCwd ?? "", term, fit, bridge };
      tabs = [...tabs, tab];
      activeId = dockId;
    } catch (e) {
      // CPE-1245: `openTerminalTab` already rolled back the dock entry it created (if any) before
      // rethrowing, so the only leftover here is the xterm instance THIS function made — dispose it so a
      // failed open never leaves an orphaned dock tab OR an undisposed `Terminal` behind. `fit` (the addon)
      // never attached — `mountXterm` only runs once the tab is in `tabs[]`, which never happened here.
      term.dispose();
      openError = e instanceof Error ? e.message : String(e);
    } finally {
      opening = false;
    }
  }

  async function selectTab(id: number): Promise<void> {
    if (id === activeId) return;
    activeId = id;
    await tick();
    const tab = tabs.find((t) => t.id === id);
    if (tab) refit(tab);
  }

  async function closeTab(id: number): Promise<void> {
    const tab = tabs.find((t) => t.id === id);
    if (!tab) return;
    tabs = tabs.filter((t) => t.id !== id);
    if (activeId === id) activeId = tabs.length > 0 ? tabs[tabs.length - 1].id : null;
    await closeTerminalTab(tab.id, tab.bridge);
    tab.term.dispose();
  }

  let bodyEl: HTMLDivElement;
  let ro: ResizeObserver | null = null;

  onMount(() => {
    void addTab(effectiveCwd);
    if (typeof ResizeObserver !== "undefined") {
      ro = new ResizeObserver(() => {
        if (activeTab) refit(activeTab);
      });
      ro.observe(bodyEl);
    }
  });

  onDestroy(() => {
    ro?.disconnect();
    // Close every still-open session — the panel-closed DoD ("no PTY/background cost when unused").
    for (const t of tabs) {
      void closeTerminalTab(t.id, t.bridge);
      t.term.dispose();
    }
  });

  // Follow navigation: re-cwd + `cd` the active tab's shell whenever the explorer's current folder changes
  // (not on the initial mount — that's `addTab`'s job).
  let lastCwd = cwd;
  $: if (cwd !== lastCwd) {
    lastCwd = cwd;
    if (followNav && activeTab && effectiveCwd) {
      const tab = activeTab;
      tab.cwd = effectiveCwd;
      void followNavigation(tab.id, tab.bridge, effectiveCwd);
    }
  }
</script>

<div class="terminal-dock" role="region" aria-label="Terminal" data-testid="terminal-panel">
  <div class="terminal-tabbar">
    {#each tabs as t (t.id)}
      <button class="tab" class:active={t.id === activeId} on:click={() => selectTab(t.id)} title={t.cwd}>
        <Icon name="terminal" size={14} />
        <span class="tab-label">{basename(t.cwd) || "shell"}</span>
        <!-- Not a real `<button>`: it lives inside the tab's own `<button>`, and HTML doesn't allow nesting
             interactive controls. Made keyboard-operable directly instead (CPE-1245) — focusable via
             tabindex="0" (was "-1", unreachable by Tab) and Enter/Space-activated like a real button, per
             the ARIA "custom button" authoring pattern. Mirrors the AI Console launcher's tab-close, which
             sidesteps the nesting problem the other way (a real `<button>` inside a non-button `.tab` div). -->
        <!-- svelte-ignore a11y-no-static-element-interactions -->
        <span
          class="tab-close"
          role="button"
          tabindex="0"
          title="Close tab"
          on:click|stopPropagation={() => closeTab(t.id)}
          on:keydown|stopPropagation={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              closeTab(t.id);
            }
          }}
        >
          <Icon name="close" size={12} />
        </span>
      </button>
    {/each}
    <button class="tab-new" data-testid="terminal-new-tab" title="New terminal at the current folder" disabled={opening} on:click={() => addTab(effectiveCwd)}>
      <Icon name="plus" size={15} />
    </button>

    <span class="terminal-tools">
      <select class="shell-picker" data-testid="terminal-shell-picker" bind:value={selectedShell} title="Shell for new tabs">
        {#each shellChoices as c (c.label)}
          <option value={c.value}>{c.label}</option>
        {/each}
      </select>
      <label class="follow-nav" title="cd the active terminal to the folder you navigate to in the explorer">
        <input type="checkbox" data-testid="terminal-follow-nav" bind:checked={followNav} />
        Follow folder
      </label>
      <button class="iconbtn" data-testid="terminal-close-panel" title="Close terminal panel" aria-label="Close terminal panel" on:click={() => dispatch("close")}>
        <Icon name="close" size={14} />
      </button>
    </span>
  </div>

  {#if openError}
    <!-- CPE-1245: surfaces an `addTab` failure inline (plain text, no modal — [[avoid-modal-permission-popups]])
         instead of silently leaving the user staring at nothing while the tab that should have opened
         doesn't exist. -->
    <div class="terminal-open-error error" data-testid="terminal-open-error">
      Couldn't open terminal: {openError}
    </div>
  {/if}

  <div class="terminal-body" data-testid="terminal-body" bind:this={bodyEl}>
    {#each tabs as t (t.id)}
      <div class="term-host" style:display={t.id === activeId ? "block" : "none"} use:mountXterm={t}></div>
    {/each}
    {#if tabs.length === 0}
      <div class="terminal-empty">
        <p>No terminal open.</p>
        <button class="tab-new-big" disabled={opening} on:click={() => addTab(effectiveCwd)}>
          <Icon name="plus" size={15} /> New Terminal
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .terminal-dock {
    display: flex;
    flex-direction: column;
    height: 240px;
    min-height: 120px;
    max-height: 70vh;
    resize: vertical;
    overflow: hidden;
    border-top: 1px solid var(--border);
    background: var(--surface);
    flex: none;
  }
  .terminal-tabbar {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .terminal-tools {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
    padding: 0 8px;
  }
  .shell-picker {
    font: inherit;
    font-size: 12px;
    color: var(--text);
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    height: 26px;
    padding: 0 4px;
  }
  .follow-nav {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--text-dim);
    white-space: nowrap;
    cursor: pointer;
  }
  .terminal-open-error {
    font-size: 12px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-alt);
    flex: none;
  }
  .terminal-body {
    flex: 1;
    min-height: 0;
    position: relative;
    background: #ffffff; /* matches the xterm `theme.background` above — a light terminal surface */
  }
  .term-host {
    position: absolute;
    inset: 0;
    padding: 4px 6px;
  }
  .terminal-empty {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--text-dim);
    font-size: 13px;
  }
  .tab-new-big {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    color: var(--text);
    background: var(--surface-alt);
  }
  .tab-new-big:hover {
    background: var(--hover);
  }
</style>
