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

  onMount(async () => {
    if (!isWindows) {
      installed = false;
      return;
    }
    try {
      installed = await invoke<boolean>("shell_integration_installed");
    } catch {
      installed = false; // treat an unreadable state as "not installed" rather than blocking the toggle
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
    color: var(--danger, #c0392b);
  }
</style>
