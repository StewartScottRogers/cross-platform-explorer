<script lang="ts">
  /**
   * Diagnostics overlay (CPE-758): a small on-screen readout of recent backend/OS calls and their
   * durations, so you can see where time goes without devtools. Fed by the instrumented `invoke`
   * wrapper via the `diagCalls` store, so it covers *every* OS resource call automatically. Only
   * rendered while Diagnostics is on (toggled from the Application menu). Purely informational —
   * pointer-events off, never intercepts clicks.
   */
  import { diagCalls, type DiagCall } from "../diagnostics";

  /** App version, for the copied report header (CPE-759). */
  export let version = "";

  const SHOWN = 12;
  $: recent = $diagCalls.slice(0, SHOWN);
  $: slowest = $diagCalls.reduce<DiagCall | null>((m, c) => (!m || c.ms > m.ms ? c : m), null);
  // Colour bands: green (fast) → amber (noticeable) → red (slow), so the eye lands on the costly calls.
  const band = (ms: number) => (ms >= 500 ? "slow" : ms >= 100 ? "warn" : "ok");

  // Copy the WHOLE recent-call buffer as readable text so it can be pasted straight back (CPE-759).
  let copied = false;
  function buildReport(): string {
    const calls = $diagCalls;
    const stamp = new Date().toISOString().replace("T", " ").slice(0, 19);
    const plat = typeof navigator !== "undefined" && navigator.platform ? ` · ${navigator.platform}` : "";
    const head = `Cross-Platform Explorer diagnostics — v${version || "?"}`;
    const meta = `${stamp} · ${calls.length} calls${slowest ? ` · slowest ${slowest.cmd} ${slowest.ms}ms` : ""}${plat}`;
    const rows = calls.map((c) => `${String(c.ms).padStart(6)}ms  ${c.cmd}${c.ok ? "" : "  (failed)"}`);
    return [head, meta, "─".repeat(30), ...rows].join("\n");
  }
  async function copyReport() {
    try {
      await navigator.clipboard.writeText(buildReport());
      copied = true;
      setTimeout(() => (copied = false), 1600);
    } catch {
      /* clipboard blocked — nothing we can surface here */
    }
  }
</script>

<aside class="diag" aria-label="Diagnostics — backend call timings">
  <div class="diag-head">
    <span>⏱ Diagnostics</span>
    {#if slowest}<span class="diag-slowest {band(slowest.ms)}">slowest {slowest.cmd} {slowest.ms}ms</span>{/if}
    <button
      class="diag-copy"
      title="Copy the diagnostics to the clipboard"
      disabled={$diagCalls.length === 0}
      on:click={copyReport}
    >{copied ? "Copied ✓" : "Copy"}</button>
  </div>
  {#if recent.length === 0}
    <div class="diag-empty">Navigate or load something — OS calls appear here.</div>
  {:else}
    <ul class="diag-list">
      {#each recent as c, i (`${c.at}-${c.cmd}-${i}`)}
        <li class:err={!c.ok}>
          <span class="diag-ms {band(c.ms)}">{c.ms}ms</span>
          <span class="diag-cmd">{c.cmd}</span>
          {#if !c.ok}<span class="diag-x" title="failed">✕</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</aside>

<style>
  .diag {
    position: fixed;
    left: 8px;
    bottom: 8px;
    z-index: 300;
    width: 300px;
    max-height: 46vh;
    display: flex;
    flex-direction: column;
    background: rgba(0, 0, 0, 0.78);
    color: #d8d8d8;
    border: 1px solid rgba(255, 255, 255, 0.16);
    border-radius: 7px;
    font-family: var(--mono, ui-monospace, Consolas, monospace);
    font-size: 11px;
    pointer-events: none; /* informational only — never eats clicks */
    overflow: hidden;
  }
  .diag-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 5px 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.12);
    font-weight: 700;
    color: #7fe0a0;
  }
  .diag-slowest {
    flex: 1;
    font-weight: 500;
    font-size: 10px;
    opacity: 0.85;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* The Copy control is interactive even though the overlay itself is click-through (CPE-759). */
  .diag-copy {
    flex: 0 0 auto;
    pointer-events: auto;
    cursor: pointer;
    padding: 1px 8px;
    border-radius: 4px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    background: rgba(255, 255, 255, 0.08);
    color: #d8d8d8;
    font: inherit;
    font-size: 10px;
  }
  .diag-copy:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.18);
  }
  .diag-copy:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .diag-empty {
    padding: 8px;
    opacity: 0.6;
  }
  .diag-list {
    list-style: none;
    margin: 0;
    padding: 3px 4px;
    overflow-y: auto;
  }
  .diag-list li {
    display: flex;
    align-items: baseline;
    gap: 7px;
    padding: 1px 4px;
  }
  .diag-list li.err {
    opacity: 0.85;
  }
  .diag-ms {
    flex: 0 0 auto;
    width: 52px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .diag-cmd {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .diag-x {
    color: #e57373;
  }
  .ok {
    color: #7fe0a0;
  }
  .warn {
    color: #e6c07b;
  }
  .slow {
    color: #e57373;
    font-weight: 700;
  }
</style>
