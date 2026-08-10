<script lang="ts">
  /** App-wide Settings dialog (CPE-229) reachable from the Application menu. It
      mirrors the app-level Toolbar gear content but as a modal. It is a dumb
      view: current values come in as props, and every change is dispatched for
      App to apply + persist, so there is a single source of truth. */
  import { createEventDispatcher, onMount } from "svelte";
  import { platformActive } from "../sidecar";
  import * as settings from "../settings";
  import { applyTheme } from "../theme";
  import type { ThemeSetting } from "../types";
  import SidecarManager from "./SidecarManager.svelte";
  import ShellIntegration from "./ShellIntegration.svelte";
  import ScheduledSnapshots from "./ScheduledSnapshots.svelte";
  import SpotlightHotkeySettings from "./SpotlightHotkeySettings.svelte";
  import ContentEmbedderSettings from "./ContentEmbedderSettings.svelte";
  import CopilotSettings from "./CopilotSettings.svelte";

  export let showHidden = false;
  export let showDetails = true;

  const dispatch = createEventDispatcher<{
    close: void;
    setHidden: boolean;
    setDetails: boolean;
    reset: void;
    openConsole: void;
  }>();

  // Whether the sidecar platform is built into this app. When off, the whole management
  // section is hidden so the plain explorer looks untouched (the delete-test, UI layer).
  let platformOn: boolean | null = null;
  onMount(async () => {
    platformOn = await platformActive();
  });

  // Appearance / theme (CPE-1536, epic CPE-1492): the visible entry point for the theme preference.
  // System / Light / Dark (CPE-1541 added Dark, once CPE-1539's palette + CPE-1540's resolveTheme
  // widening both landed). Kept as an array so it stays a one-line change to extend further. An inline
  // instant control ([[prefer-inline-instant-controls]]), self-contained like the other rows on this
  // page: reads/writes settings.ts directly and also calls applyTheme so the data-theme attribute
  // updates live, same instant-apply feel as the toggles below.
  const THEME_OPTIONS: { value: ThemeSetting; label: string }[] = [
    { value: "system", label: "System" },
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
  ];
  let theme = settings.loadTheme();
  function setTheme(v: ThemeSetting) {
    theme = v;
    settings.saveTheme(v);
    applyTheme(v);
  }
  function onThemeChange(e: Event) {
    setTheme((e.currentTarget as HTMLSelectElement).value as ThemeSetting);
  }

  // Native-bridge opt-in (CPE-1177, epic CPE-717): OFF by default. When on, it reveals the OS-native
  // tag/comment Pull/Push controls in TagEditor and the read-only Native metadata section in
  // PropertiesDialog (CPE-1176). A Settings control, never a launch-time permission modal
  // ([[avoid-modal-permission-popups]]). Self-contained (reads/writes settings.ts directly), like the
  // other settings rows on this page.
  let nativeBridgeEnabled = settings.loadNativeBridgeEnabled();
  function setNativeBridgeEnabled(on: boolean) {
    nativeBridgeEnabled = on;
    settings.saveNativeBridgeEnabled(on);
  }

  // Encrypted vaults (CPE-1250, epic CPE-738): the DEFAULT for the create dialog's "Remember passphrase in
  // the keychain" checkbox. Off by default — a passphrase persists in the OS keychain only when the user
  // opts in. Self-contained (reads/writes settings.ts directly), like the native-bridge row above.
  let vaultRememberPassphrases = settings.loadVaultRememberPassphrases();
  function setVaultRememberPassphrases(on: boolean) {
    vaultRememberPassphrases = on;
    settings.saveVaultRememberPassphrases(on);
  }

  // Close-to-tray (CPE-1272, epic CPE-713): when on, closing the main window HIDES it (the app stays in
  // the system tray) instead of quitting. Off by default so a plain close still quits — Quit stays on the
  // tray menu. The Rust close handler reads this flag from settings.json; nothing else to invoke here.
  let closeToTray = settings.loadCloseToTray();
  function setCloseToTray(on: boolean) {
    closeToTray = on;
    settings.saveCloseToTray(on);
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Settings" on:click|stopPropagation>
    <h2>Settings</h2>

    <div class="settings-row">
      <span>Show details/preview pane</span>
      <input
        type="checkbox"
        checked={showDetails}
        on:change={(e) => dispatch("setDetails", e.currentTarget.checked)}
      />
    </div>
    <div class="settings-row">
      <span>Show hidden files</span>
      <input
        type="checkbox"
        checked={showHidden}
        on:change={(e) => dispatch("setHidden", e.currentTarget.checked)}
      />
    </div>
    <div class="settings-row">
      <button class="settings-btn" on:click={() => dispatch("reset")}>
        Reset all settings to defaults
      </button>
    </div>

    <div class="section-title">Appearance</div>
    <div class="settings-row">
      <span>Theme</span>
      <select data-testid="theme-select" value={theme} on:change={onThemeChange}>
        {#each THEME_OPTIONS as opt (opt.value)}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </div>
    <div class="note">
      Choose Light or Dark, or System to follow your OS automatically.
    </div>

    <div class="section-title">Native metadata bridge</div>
    <div class="settings-row">
      <span>Sync tags with OS-native file metadata</span>
      <input
        type="checkbox"
        checked={nativeBridgeEnabled}
        data-testid="native-bridge-toggle"
        on:change={(e) => setNativeBridgeEnabled(e.currentTarget.checked)}
      />
    </div>
    <div class="note">
      Adds Pull/Push controls to the tag editor and a read-only Native metadata section to Properties.
      Off by default.
    </div>

    <div class="section-title">Encrypted vaults</div>
    <div class="settings-row">
      <span>Remember vault passphrases in the OS keychain</span>
      <input
        type="checkbox"
        checked={vaultRememberPassphrases}
        data-testid="vault-remember-toggle"
        on:change={(e) => setVaultRememberPassphrases(e.currentTarget.checked)}
      />
    </div>
    <div class="note">
      Sets the default for the "Remember passphrase" option when you create a vault. When on, a vault's
      passphrase is saved in this device's secure keychain so you don't have to retype it. Off by default —
      turning it off keeps passphrases in memory only, for the current session.
    </div>

    <div class="section-title">Scheduled snapshots</div>
    <div class="note">
      Automatically capture a snapshot of a folder on an interval and keep a retention window. Opt-in per
      folder and off by default — nothing is captured until you add an enabled folder below.
    </div>
    <ScheduledSnapshots />

    <div class="section-title">System tray</div>
    <div class="settings-row">
      <span>Close to tray instead of quitting</span>
      <input
        type="checkbox"
        checked={closeToTray}
        data-testid="close-to-tray-toggle"
        on:change={(e) => setCloseToTray(e.currentTarget.checked)}
      />
    </div>
    <div class="note">
      When on, closing the window keeps Cross-Platform Explorer running in the system tray — click the tray
      icon to bring it back, or use the tray menu's Quit to exit. The tray menu also lists your recent and
      pinned folders for a one-click jump. Off by default: closing the window quits the app.
    </div>

    <ShellIntegration />

    <SpotlightHotkeySettings />

    <ContentEmbedderSettings />

    <CopilotSettings />

    {#if platformOn}
      <div class="section-title">Sidecar platform</div>
      <SidecarManager on:openConsole={() => dispatch("openConsole")} />
    {/if}

    <div class="actions">
      <button class="btn primary" on:click={() => dispatch("close")}>Close</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    display: grid;
    place-items: center;
    z-index: 200;
  }
  .dialog {
    width: 420px;
    max-width: 90vw;
    background: var(--surface);
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 12px; }
  .section-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    margin: 16px 0 6px;
  }
  .note {
    font-size: 12px;
    color: var(--text-dim);
    margin-top: 2px;
  }
  .settings-row select {
    height: 28px;
    padding: 0 8px;
    font: inherit;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
  }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover { background: var(--accent-hover); }
</style>
