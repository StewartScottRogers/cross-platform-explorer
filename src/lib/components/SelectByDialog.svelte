<script lang="ts">
  /**
   * "Select by…" criteria dialog (CPE-782, epic CPE-711). Build a CPE-774 `Condition` across every kind
   * (extension / glob / size / age / is-folder); App applies it via `selectMatching` (CPE-780) to set the
   * selection. Richer than the glob-only "Select by pattern" — a thin front-end over the tested matcher.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import type { Condition } from "../colorRules";

  const dispatch = createEventDispatcher<{ submit: Condition; cancel: void }>();

  let kind: Condition["kind"] = "ext";
  let exts = "";
  let glob = "";
  let sizeMin = "";
  let sizeMax = "";
  let days = "7";
  let isDirValue = true;
  let firstField: HTMLElement;

  onMount(() => firstField?.focus());

  function buildCondition(): Condition | null {
    switch (kind) {
      case "ext": {
        const parts = exts.split(",").map((s) => s.trim()).filter(Boolean);
        return parts.length ? { kind: "ext", exts: parts } : null;
      }
      case "glob":
        return glob.trim() ? { kind: "glob", pattern: glob.trim() } : null;
      case "size": {
        const min = sizeMin.trim() === "" ? undefined : Number(sizeMin);
        const max = sizeMax.trim() === "" ? undefined : Number(sizeMax);
        if ((min !== undefined && Number.isNaN(min)) || (max !== undefined && Number.isNaN(max))) return null;
        if (min === undefined && max === undefined) return null;
        return { kind: "size", min, max };
      }
      case "olderThan":
      case "newerThan": {
        const d = Number(days);
        return Number.isFinite(d) && d > 0 ? { kind, days: d } : null;
      }
      case "isDir":
        return { kind: "isDir", value: isDirValue };
    }
  }

  function submit() {
    const c = buildCondition();
    if (c) dispatch("submit", c);
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Select by criteria" on:click|stopPropagation>
    <h2>Select by…</h2>
    <p>Select every visible item matching a criterion.</p>

    <div class="row">
      <select bind:this={firstField} bind:value={kind} aria-label="Criterion kind">
        <option value="ext">Extension</option>
        <option value="glob">Name (glob)</option>
        <option value="size">Size</option>
        <option value="olderThan">Older than</option>
        <option value="newerThan">Newer than</option>
        <option value="isDir">Is folder</option>
      </select>

      {#if kind === "ext"}
        <input class="grow" placeholder="ts, md, png" bind:value={exts} aria-label="Extensions" on:keydown={(e) => e.key === "Enter" && submit()} />
      {:else if kind === "glob"}
        <input class="grow" placeholder="*.min.js" bind:value={glob} aria-label="Glob pattern" on:keydown={(e) => e.key === "Enter" && submit()} />
      {:else if kind === "size"}
        <input class="num" placeholder="min bytes" bind:value={sizeMin} aria-label="Min bytes" />
        <input class="num" placeholder="max bytes" bind:value={sizeMax} aria-label="Max bytes" />
      {:else if kind === "olderThan" || kind === "newerThan"}
        <input class="num" bind:value={days} aria-label="Days" /> <span class="unit">days</span>
      {:else if kind === "isDir"}
        <label class="chk"><input type="checkbox" bind:checked={isDirValue} /> folder</label>
      {/if}
    </div>

    <div class="actions">
      <button class="btn" on:click={() => dispatch("cancel")}>Cancel</button>
      <button class="btn primary" data-testid="select-btn" on:click={submit}>Select</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 460px; max-width: 92vw; background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 16px; margin-bottom: 8px; }
  p { color: var(--text-dim); font-size: 12.5px; margin-bottom: 12px; }
  .row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .row .grow { flex: 1 1 120px; }
  select, input:not([type="checkbox"]) { height: 32px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); }
  .num { width: 100px; }
  .unit, .chk { font-size: 12.5px; color: var(--text-dim); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .btn { height: 32px; padding: 0 16px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover { background: var(--accent-hover); }
</style>
