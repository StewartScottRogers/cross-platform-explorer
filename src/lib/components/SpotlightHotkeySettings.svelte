<script lang="ts">
  /** Settings › Spotlight global hotkey (CPE-1215, epic CPE-704). A self-contained toggle + chord field
      that claims/releases an OS-wide hotkey (tauri-plugin-global-shortcut) which fires the `spotlight:open`
      event the CPE-1216 overlay listens for — so Spotlight can be summoned even while the main window is
      hidden/unfocused. Self-managing (owns its own register/unregister calls + persistence via settings.ts)
      like ShellIntegration, so the parent SettingsDialog stays a dumb view. Off by default: toggling ON
      registers with the OS, toggling OFF unregisters cleanly (no background cost when off). No launch-time
      consent modal — this control lives only in Settings ([[avoid-modal-permission-popups]]).

      OS-gated honesty: the chord actually firing while the window is hidden is NOT something this
      component (or any headless test) can verify — only that the register/unregister backend calls
      succeed and the setting round-trips. Real key-press verification needs an attended desktop run. */
  import { invoke } from "../invoke";
  import * as settings from "../settings";

  let enabled = settings.loadSpotlightHotkeyEnabled();
  let chord = settings.loadSpotlightHotkeyChord();
  let registeredChord = enabled ? chord : ""; // the chord currently claimed with the OS, if any
  let busy = false;
  let error = "";

  async function setEnabled(on: boolean) {
    busy = true;
    error = "";
    try {
      if (on) {
        await invoke("register_spotlight_hotkey", { chord });
        registeredChord = chord;
        enabled = true;
      } else {
        if (registeredChord) await invoke("unregister_spotlight_hotkey", { chord: registeredChord });
        registeredChord = "";
        enabled = false;
      }
      settings.saveSpotlightHotkeyEnabled(enabled);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      enabled = registeredChord !== ""; // reflect whatever actually ended up registered
    } finally {
      busy = false;
    }
  }

  async function applyChord() {
    const next = chord.trim();
    if (!next) {
      chord = registeredChord || settings.DEFAULT_SPOTLIGHT_HOTKEY_CHORD;
      return;
    }
    if (next === registeredChord) return;
    if (!enabled) {
      // Not registered yet — just persist the new chord for the next time it's enabled.
      chord = next;
      settings.saveSpotlightHotkeyChord(chord);
      return;
    }
    busy = true;
    error = "";
    try {
      await invoke("register_spotlight_hotkey", { chord: next });
      if (registeredChord) await invoke("unregister_spotlight_hotkey", { chord: registeredChord });
      registeredChord = next;
      chord = next;
      settings.saveSpotlightHotkeyChord(chord);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      chord = registeredChord; // revert the field to the chord that's actually still claimed
    } finally {
      busy = false;
    }
  }
</script>

<div class="section-title">Spotlight global hotkey</div>
<div class="settings-row">
  <span>Open Spotlight with a global keyboard shortcut</span>
  <input
    type="checkbox"
    checked={enabled}
    disabled={busy}
    data-testid="spotlight-hotkey-toggle"
    on:change={(e) => setEnabled(e.currentTarget.checked)}
  />
</div>
<div class="settings-row">
  <span>Shortcut</span>
  <input
    type="text"
    class="chord-input"
    bind:value={chord}
    disabled={busy}
    data-testid="spotlight-hotkey-chord"
    on:blur={applyChord}
    on:keydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
  />
</div>
{#if error}
  <div class="note error">Couldn’t update: {error}</div>
{:else}
  <div class="note">
    Works even when the window is hidden. Off by default — no shortcut is claimed with the OS until
    enabled here.
  </div>
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
  .chord-input {
    width: 220px;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
    font-size: 12px;
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
