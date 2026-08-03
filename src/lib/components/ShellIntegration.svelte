<script lang="ts">
  /** Settings › Shell integration (CPE-1023, epic CPE-712). A self-contained toggle that adds/removes the
      "Open in Cross-Platform Explorer" entry in the OS right-click menu by calling the backend
      install/uninstall commands (CPE-1020). Self-managing (queries its own state on mount) like
      SidecarManager, so the parent SettingsDialog stays a dumb view. Windows-only backend today: on other
      OSes the control is shown disabled with a "coming soon" note rather than hidden, so the feature is
      discoverable everywhere. No launch-time consent modal — this lives in Settings per
      [[avoid-modal-permission-popups]]. */
  import { onMount } from "svelte";
  import { invoke } from "../invoke";

  // The webview's navigator reflects the host OS (as DiagnosticsOverlay already relies on). The apply glue
  // exists on Windows only so far (CPE-1020); elsewhere we disable the control and say so.
  const platform = typeof navigator !== "undefined" ? navigator.platform || navigator.userAgent : "";
  const isWindows = /win/i.test(platform);
  const osLabel = /mac/i.test(platform) ? "macOS" : /linux/i.test(platform) ? "Linux" : "this platform";

  let installed: boolean | null = null; // null → still loading / unknown
  let busy = false;
  let error = "";

  // Default-apps registration (CPE-1277): a separate, honest control. We can only REGISTER CPE as a
  // Windows Default-apps candidate — never force the default — so the copy directs the user to confirm.
  let registered: boolean | null = null; // null → still loading / unknown
  let dfmBusy = false;
  let dfmError = "";

  onMount(async () => {
    if (!isWindows) {
      installed = false;
      registered = false;
      return;
    }
    try {
      installed = await invoke<boolean>("shell_integration_installed");
    } catch {
      installed = false; // treat an unreadable state as "not installed" rather than blocking the toggle
    }
    try {
      registered = await invoke<boolean>("default_file_manager_status");
    } catch {
      registered = false;
    }
  });

  async function toggle(on: boolean) {
    busy = true;
    error = "";
    try {
      await invoke(on ? "install_shell_integration" : "uninstall_shell_integration");
      installed = await invoke<boolean>("shell_integration_installed");
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      // Re-read so the checkbox reflects reality after a partial failure.
      try {
        installed = await invoke<boolean>("shell_integration_installed");
      } catch {
        /* keep the last known value */
      }
    } finally {
      busy = false;
    }
  }

  async function setDefault() {
    dfmBusy = true;
    dfmError = "";
    try {
      // Registers CPE + opens Windows Settings → Default apps; it never force-sets the default.
      await invoke("set_default_file_manager");
      registered = await invoke<boolean>("default_file_manager_status");
    } catch (e) {
      dfmError = e instanceof Error ? e.message : String(e);
      try {
        registered = await invoke<boolean>("default_file_manager_status");
      } catch {
        /* keep the last known value */
      }
    } finally {
      dfmBusy = false;
    }
  }

  async function unsetDefault() {
    dfmBusy = true;
    dfmError = "";
    try {
      await invoke("unset_default_file_manager");
      registered = await invoke<boolean>("default_file_manager_status");
    } catch (e) {
      dfmError = e instanceof Error ? e.message : String(e);
      try {
        registered = await invoke<boolean>("default_file_manager_status");
      } catch {
        /* keep the last known value */
      }
    } finally {
      dfmBusy = false;
    }
  }
</script>

<div class="section-title">Shell integration</div>
<div class="settings-row">
  <span>Add “Open in Cross-Platform Explorer” to the right-click menu</span>
  <input
    type="checkbox"
    checked={installed === true}
    disabled={!isWindows || busy || installed === null}
    on:change={(e) => toggle(e.currentTarget.checked)}
  />
</div>
{#if !isWindows}
  <div class="note">Coming to {osLabel} soon — available on Windows today.</div>
{:else if error}
  <div class="note error">Couldn’t update: {error}</div>
{/if}

<div class="settings-row dfm">
  <div class="dfm-text">
    <span>Set as default file manager</span>
    <div class="note">
      {#if isWindows}
        Registers Cross-Platform Explorer with Windows; you then confirm it in Settings → Default apps.
        Windows never lets an app set itself as the default — this only makes CPE selectable there.
      {:else}
        Default-apps registration is Windows-only for now — {osLabel} support is on the way.
      {/if}
    </div>
  </div>
  <div class="dfm-actions">
    <button
      type="button"
      class="btn"
      disabled={!isWindows || dfmBusy || registered === null}
      on:click={setDefault}
    >
      {registered ? "Re-register" : "Register"}
    </button>
    <button
      type="button"
      class="btn"
      disabled={!isWindows || dfmBusy || registered !== true}
      on:click={unsetDefault}
    >
      Unregister
    </button>
  </div>
</div>
{#if isWindows && registered === true}
  <div class="note">Registered — confirm the choice in Windows Settings → Default apps.</div>
{/if}
{#if isWindows && dfmError}
  <div class="note error">Couldn’t update: {dfmError}</div>
{/if}

<style>
  .section-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    margin: 16px 0 6px;
  }
  .settings-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 0;
  }
  .note {
    font-size: 12px;
    color: var(--text-dim);
    margin-top: 2px;
  }
  .note.error {
    color: var(--danger);
  }
  .settings-row.dfm {
    align-items: flex-start;
  }
  .dfm-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .dfm-text .note {
    margin-top: 0;
  }
  .dfm-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    flex: 0 0 auto;
  }
  .btn {
    font: inherit;
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
  }
  .btn:hover:not(:disabled) {
    background: var(--hover);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
