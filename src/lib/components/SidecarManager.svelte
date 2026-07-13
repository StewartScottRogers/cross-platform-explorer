<script lang="ts">
  /**
   * Platform management panel (CPE-274): the user's control surface over which
   * Mega-Feature sidecars are active. Lists each registered sidecar with its version,
   * contract compatibility and running state, an enable/disable toggle (start/stop), and
   * its granted capabilities with per-capability revoke (CPE-296). Disabling one sidecar
   * leaves the explorer and other sidecars untouched.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import {
    sidecarDetails,
    setEnabled,
    stopSidecar,
    revokeCapability,
    CAPABILITY_INFO,
    type SidecarInfo,
    type Capability,
  } from "../sidecar";

  const dispatch = createEventDispatcher<{ openConsole: void }>();

  let rows: SidecarInfo[] | null = null;

  async function refresh() {
    rows = await sidecarDetails();
  }
  onMount(refresh);

  async function toggleEnabled(row: SidecarInfo) {
    await setEnabled(row.id, !row.enabled);
    await refresh();
  }

  async function stop(row: SidecarInfo) {
    await stopSidecar(row.id);
    await refresh();
  }

  async function revoke(row: SidecarInfo, cap: Capability) {
    await revokeCapability(row.id, cap);
    await refresh();
  }
</script>

<div class="mgr">
  {#if rows === null}
    <div class="muted">Checking…</div>
  {:else if rows.length === 0}
    <div class="muted">No sidecars registered.</div>
  {:else}
    {#each rows as row (row.id)}
      <div class="sidecar" class:disabled={!row.enabled}>
        <div class="head">
          <span class="dot" class:on={row.running} title={row.running ? "Running" : "Stopped"} />
          <span class="name">{row.name}</span>
          <span class="ver">v{row.version}</span>
          <span class="compat" class:bad={!row.compatible} title="Contract version">
            contract {row.contract}{row.compatible ? "" : " (incompatible)"}
          </span>
          <span class="spacer" />
          <label class="switch" title={row.enabled ? "Enabled" : "Disabled"}>
            <input type="checkbox" checked={row.enabled} on:change={() => toggleEnabled(row)} />
            <span>{row.enabled ? "Enabled" : "Disabled"}</span>
          </label>
        </div>

        <div class="caps">
          {#each row.requested as cap (cap)}
            {@const isGranted = row.granted.includes(cap)}
            <span class="cap" class:granted={isGranted}>
              {CAPABILITY_INFO[cap].label}
              {#if isGranted}
                <button class="revoke" title="Revoke" on:click={() => revoke(row, cap)}>×</button>
              {:else}
                <span class="denied">denied</span>
              {/if}
            </span>
          {/each}
          {#if row.requested.length === 0}<span class="muted">no capabilities</span>{/if}
        </div>

        {#if row.id === "ai-console"}
          <div class="row-actions">
            <button
              class="settings-btn"
              disabled={!row.enabled}
              on:click={() => dispatch("openConsole")}
            >
              Open
            </button>
            {#if row.running}
              <button class="settings-btn" on:click={() => stop(row)}>Stop</button>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  .mgr {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .muted {
    color: var(--text-dim, #a0a0a0);
    font-size: 13px;
  }
  .sidecar {
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 8px;
    padding: 10px 12px;
    background: var(--bg, #171717);
  }
  .sidecar.disabled {
    opacity: 0.6;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-dim, #777);
    flex: 0 0 auto;
  }
  .dot.on {
    background: #3a9d4a;
    box-shadow: 0 0 0 2px rgba(58, 157, 74, 0.25);
  }
  .name {
    font-weight: 600;
  }
  .ver,
  .compat {
    color: var(--text-dim, #a0a0a0);
    font-size: 12px;
  }
  .compat.bad {
    color: var(--warn, #d08b2b);
  }
  .spacer {
    flex: 1;
  }
  .switch {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    cursor: pointer;
  }
  .caps {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }
  .cap {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 999px;
    border: 1px solid var(--border, #3a3a3a);
    color: var(--text-dim, #a0a0a0);
  }
  .cap.granted {
    color: var(--text, #eaeaea);
    border-color: var(--accent, #3a7d3a);
  }
  .denied {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-dim, #888);
  }
  .revoke {
    border: 0;
    background: transparent;
    color: var(--text-dim, #a0a0a0);
    cursor: pointer;
    font-size: 13px;
    line-height: 1;
    padding: 0 2px;
  }
  .revoke:hover {
    color: var(--warn, #d08b2b);
  }
  .row-actions {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
</style>
