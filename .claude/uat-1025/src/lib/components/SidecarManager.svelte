<script lang="ts">
  /**
   * Platform management panel (CPE-274): the user's control surface over which
   * Mega-Feature sidecars are active. Lists each registered sidecar with its version,
   * contract compatibility and running state, an enable/disable toggle (start/stop), and
   * its granted capabilities with per-capability revoke (CPE-296). Disabling one sidecar
   * leaves the explorer and other sidecars untouched.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { t } from "../i18n";
  import {
    sidecarDetails,
    setEnabled,
    stopSidecar,
    revokeCapability,
    setConsent,
    repairSidecar,
    sidecarDiagnostics,
    CAPABILITY_INFO,
    type SidecarInfo,
    type Capability,
    type SidecarDiagnostics,
  } from "../sidecar";

  const dispatch = createEventDispatcher<{ openConsole: void }>();

  let rows: SidecarInfo[] | null = null;
  // Per-sidecar diagnostics (CPE-323), keyed by id; loaded on refresh + when logs opened.
  let diags: Record<string, SidecarDiagnostics> = {};
  // Which sidecars have their log panel expanded.
  let logsOpen: Record<string, boolean> = {};
  // Transient per-sidecar repair outcome message (CPE-863), keyed by id.
  let repairMsg: Record<string, string> = {};

  /** A sidecar's health status (CPE-863) — a status key + tone, worst-first, derived from binary
   *  presence, contract compat, enablement, the last error, and running state. */
  function statusOf(row: SidecarInfo, diag?: SidecarDiagnostics): { key: string; tone: string } {
    if (!row.binary_ok) return { key: "stMissing", tone: "bad" };
    if (!row.compatible) return { key: "stIncompatible", tone: "bad" };
    if (!row.enabled) return { key: "disabled", tone: "idle" }; // reuse mgr.disabled
    if (diag?.last_error && !row.running) return { key: "stError", tone: "warn" };
    if (row.running) return { key: "running", tone: "ok" }; // reuse mgr.running
    return { key: "stReady", tone: "idle" };
  }

  async function loadDiag(id: string) {
    diags = { ...diags, [id]: await sidecarDiagnostics(id) };
  }

  async function refresh() {
    rows = await sidecarDetails();
    // Refresh diagnostics for every registered sidecar so the health line is current.
    await Promise.all(rows.map((r) => loadDiag(r.id)));
  }
  onMount(refresh);

  async function toggleLogs(row: SidecarInfo) {
    const open = !logsOpen[row.id];
    logsOpen = { ...logsOpen, [row.id]: open };
    if (open) await loadDiag(row.id); // pull the freshest lines when opening
  }

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

  // Grant a denied/undecided capability from Settings (CPE-860) — this is now the place consent is
  // given, instead of a launch-time popup. Takes effect on the sidecar's next launch.
  async function grant(row: SidecarInfo, cap: Capability) {
    const granted = [...row.granted, cap];
    await setConsent(row.id, granted, granted);
    await refresh();
  }

  // One-click repair (CPE-863): reap orphan daemons, clear a wedged connection + stored error, re-check
  // the binary. Shows the steps taken, then refreshes so the status pill reflects the new state.
  async function repair(row: SidecarInfo) {
    const r = await repairSidecar(row.id);
    repairMsg = { ...repairMsg, [row.id]: r ? r.actions.join("; ") : $t("mgr.repairFailed") };
    await refresh();
  }
</script>

<div class="mgr">
  {#if rows === null}
    <div class="muted">{$t("mgr.checking")}</div>
  {:else if rows.length === 0}
    <div class="muted">{$t("mgr.none")}</div>
  {:else}
    {#each rows as row (row.id)}
      {@const diag = diags[row.id]}
      {@const health = statusOf(row, diag)}
      <div class="sidecar" class:disabled={!row.enabled}>
        <div class="head">
          <span class="dot" class:on={row.running} title={row.running ? $t("mgr.running") : $t("mgr.stopped")} />
          <span class="name">{row.name}</span>
          <span class="ver">v{row.version}</span>
          <span class="status {health.tone}">{$t("mgr." + health.key)}</span>
          <span class="compat" class:bad={!row.compatible} title={$t("mgr.contractTip")}>
            {row.compatible ? $t("mgr.contractOk", { v: row.contract }) : $t("mgr.contractBad", { v: row.contract })}
          </span>
          <span class="spacer" />
          <label class="switch" title={row.enabled ? $t("mgr.enabled") : $t("mgr.disabled")}>
            <input type="checkbox" checked={row.enabled} on:change={() => toggleEnabled(row)} />
            <span>{row.enabled ? $t("mgr.enabled") : $t("mgr.disabled")}</span>
          </label>
        </div>

        <div class="caps">
          {#each row.requested as cap (cap)}
            {@const isGranted = row.granted.includes(cap)}
            <span class="cap" class:granted={isGranted}>
              {CAPABILITY_INFO[cap].label}
              {#if isGranted}
                <button class="revoke" title={$t("mgr.revoke")} on:click={() => revoke(row, cap)}>×</button>
              {:else}
                <button class="grant" title={$t("mgr.grantTip")} on:click={() => grant(row, cap)}>{$t("mgr.grant")}</button>
              {/if}
            </span>
          {/each}
          {#if row.requested.length === 0}<span class="muted">{$t("mgr.noCapabilities")}</span>{/if}
        </div>

        <div class="health">
          {#if diag?.last_error}
            <span class="err" title={$t("mgr.lastError")}>⚠ {diag.last_error}</span>
          {:else if row.running}
            <span class="ok">{$t("mgr.healthy")}</span>
          {:else}
            <span class="muted">{$t("mgr.notRunning")}</span>
          {/if}
          <span class="spacer" />
          <button class="logs-toggle repair" on:click={() => repair(row)}>{$t("mgr.repair")}</button>
          {#if diag && diag.logs.length > 0}
            <button class="logs-toggle" on:click={() => toggleLogs(row)}>
              {logsOpen[row.id] ? $t("mgr.hideLogs") : $t("mgr.viewLogs", { count: diag.logs.length })}
            </button>
          {:else}
            <span class="muted small">{$t("mgr.noLogs")}</span>
          {/if}
        </div>

        {#if repairMsg[row.id]}
          <div class="repair-msg" role="status">{$t("mgr.repairDid")}: {repairMsg[row.id]}</div>
        {/if}

        {#if logsOpen[row.id] && diag}
          <pre class="logs">{#each diag.logs as line}<span class="log-line log-{line.level}">{line.level}: {line.message}
</span>{/each}</pre>
        {/if}

        {#if row.id === "ai-console"}
          <div class="row-actions">
            <button
              class="settings-btn"
              disabled={!row.enabled}
              on:click={() => dispatch("openConsole")}
            >
              {$t("mgr.open")}
            </button>
            {#if row.running}
              <button class="settings-btn" on:click={() => stop(row)}>{$t("mgr.stop")}</button>
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
  .status {
    flex: 0 0 auto;
    white-space: nowrap;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.02em;
    padding: 1px 7px;
    border-radius: 999px;
    border: 1px solid var(--border, #3a3a3a);
    color: var(--text-dim, #a0a0a0);
  }
  .status.ok {
    color: #3a9d4a;
    border-color: #3a9d4a;
  }
  .status.warn {
    color: var(--warn, #d08b2b);
    border-color: var(--warn, #d08b2b);
  }
  .status.bad {
    color: #fff;
    background: var(--danger, #c0392b);
    border-color: var(--danger, #c0392b);
  }
  .logs-toggle.repair {
    color: var(--accent, #3a7d3a);
    border-color: var(--accent, #3a7d3a);
  }
  .repair-msg {
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-dim, #a0a0a0);
    border-left: 2px solid var(--accent, #3a7d3a);
    padding-left: 8px;
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
    flex: 0 0 auto;
    white-space: nowrap; /* tick-tack: pill keeps its label on one line; .caps wraps the pills */
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
  .grant {
    border: 1px solid var(--accent, #3a7d3a);
    background: transparent;
    color: var(--text, #eaeaea);
    cursor: pointer;
    font-size: 10px;
    line-height: 1;
    padding: 1px 6px;
    border-radius: 999px;
  }
  .grant:hover {
    background: var(--accent, #3a7d3a);
    color: #fff;
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
  .health {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    font-size: 12px;
  }
  .health .ok {
    color: #3a9d4a;
  }
  .health .err {
    color: var(--warn, #d08b2b);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 60%;
  }
  .small {
    font-size: 11px;
  }
  .logs-toggle {
    border: 1px solid var(--border, #3a3a3a);
    background: transparent;
    color: var(--text-dim, #a0a0a0);
    cursor: pointer;
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 6px;
  }
  .logs-toggle:hover {
    color: var(--text, #eaeaea);
  }
  .logs {
    margin: 8px 0 0;
    padding: 8px;
    max-height: 180px;
    overflow: auto;
    background: var(--bg-dim, #0f0f0f);
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 6px;
    font-size: 11px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .log-line {
    color: var(--text-dim, #a0a0a0);
  }
  .log-error {
    color: var(--warn, #d08b2b);
  }
  .log-warn {
    color: #c9a227;
  }
</style>
