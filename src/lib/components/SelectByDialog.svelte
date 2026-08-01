<script lang="ts">
  /**
   * "Select by…" criteria dialog (CPE-782, epic CPE-711). Build a CPE-774 `Condition` across every kind
   * (extension / glob / size / age / is-folder); App applies it via `selectMatching` (CPE-780) to set the
   * selection. Richer than the glob-only "Select by pattern" — a thin front-end over the tested matcher.
   *
   * CPE-1229 (epic CPE-978): the SAME built condition can instead be captured as a named `SavedSearch`
   * (one condition, `match: "all"`) — "the current structured search" this app has, since there's no
   * separate multi-condition search bar. `autoReveal` lets the caller open straight into that flow (the
   * command-palette "Save search…" entry) instead of making the user find a second button first.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import type { Condition } from "../colorRules";

  /** Open with the "Save search…" name field already revealed (command palette "Save search…" entry),
      vs. the default "Select by…" entry which opens on the criterion picker. */
  export let autoReveal = false;

  const dispatch = createEventDispatcher<{ submit: Condition; cancel: void; save: { name: string; condition: Condition } }>();

  let kind: Condition["kind"] = "ext";
  let exts = "";
  let glob = "";
  let sizeMin = "";
  let sizeMax = "";
  let days = "7";
  let isDirValue = true;
  let firstField: HTMLElement;

  let showSaveName = autoReveal;
  let saveName = "";
  let nameField: HTMLElement;

  onMount(() => (showSaveName ? nameField : firstField)?.focus());

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

  /** First click reveals the name field (inline, not a second modal — the criterion above is still
      live); a second click (or Enter in the name field) actually saves. */
  function saveSearch() {
    if (!showSaveName) {
      showSaveName = true;
      return;
    }
    const n = saveName.trim();
    const c = buildCondition();
    if (!n || !c) return;
    dispatch("save", { name: n, condition: c });
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

    <!-- CPE-1229: capture this same condition as a named saved search instead of (or as well as)
         applying it to the current selection. Inline reveal, not a second modal (prefer-inline-instant-controls). -->
    <div class="row save-row">
      {#if showSaveName}
        <input
          bind:this={nameField}
          class="grow"
          placeholder="Search name…"
          bind:value={saveName}
          aria-label="Search name"
          on:keydown={(e) => e.key === "Enter" && saveSearch()}
        />
        <button class="btn" data-testid="save-search-confirm" on:click={saveSearch}>Save search</button>
      {:else}
        <button class="btn ghost" data-testid="save-search-reveal" on:click={saveSearch}>Save search…</button>
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
  .dialog { width: 460px; max-width: 92vw; background: var(--surface); border: 1px solid var(--dialog-border); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 16px; margin-bottom: 8px; }
  p { color: var(--text-dim); font-size: 12.5px; margin-bottom: 12px; }
  .row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .row .grow { flex: 1 1 120px; }
  select, input:not([type="checkbox"]) { height: 32px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); }
  .num { width: 100px; }
  .unit, .chk { font-size: 12.5px; color: var(--text-dim); }
  .save-row { margin-top: 10px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .btn { height: 32px; padding: 0 16px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover { background: var(--accent-hover); }
  .btn.ghost { border-color: transparent; background: transparent; color: var(--text-dim); padding: 0 4px; height: auto; }
  .btn.ghost:hover { color: var(--text); }
</style>
