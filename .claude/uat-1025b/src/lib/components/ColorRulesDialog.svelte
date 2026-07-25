<script lang="ts">
  /**
   * Rules editor for file coloring & labels (CPE-776, epic CPE-709). A thin CRUD over `colorRulesStore`:
   * add / edit (colour, label, enable) / remove / reorder rules across the CPE-774 condition kinds. Emits
   * `change` live on every edit (so the file list previews immediately), `save` on Done (persist), and
   * `cancel` to revert. Condition building for a new rule uses a kind picker + kind-specific inputs.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { addRule, updateRule, removeRule, moveRule, toggleRule } from "../colorRulesStore";
  import type { ColorRule, Condition } from "../colorRules";

  export let rules: ColorRule[] = [];

  const dispatch = createEventDispatcher<{ change: ColorRule[]; save: ColorRule[]; cancel: void }>();

  // Working copy; every mutation re-assigns it and previews live.
  let list: ColorRule[] = rules.map((r) => ({ ...r }));

  // New-rule form state.
  let kind: Condition["kind"] = "ext";
  let exts = "";
  let glob = "";
  let sizeMin = "";
  let sizeMax = "";
  let days = "7";
  let isDirValue = true;
  let newColor = "#e2504b";
  let newLabel = "";

  function preview() {
    dispatch("change", list);
  }

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

  function add() {
    const cond = buildCondition();
    if (!cond) return;
    list = addRule(list, cond, { color: newColor, label: newLabel.trim() || undefined });
    exts = glob = sizeMin = sizeMax = newLabel = "";
    preview();
  }

  function summarize(c: Condition): string {
    switch (c.kind) {
      case "ext": return `extension: ${c.exts.join(", ")}`;
      case "glob": return `name matches ${c.pattern}`;
      case "size": {
        const p: string[] = [];
        if (c.min !== undefined) p.push(`≥ ${c.min}`);
        if (c.max !== undefined) p.push(`≤ ${c.max}`);
        return `size ${p.join(" and ")} bytes`;
      }
      case "olderThan": return `older than ${c.days} days`;
      case "newerThan": return `newer than ${c.days} days`;
      case "isDir": return c.value ? "is a folder" : "is a file";
    }
  }

  const setColor = (id: string, color: string) => { list = updateRule(list, id, { color }); preview(); };
  const setLabel = (id: string, label: string) => { list = updateRule(list, id, { label }); preview(); };
  const toggle = (id: string) => { list = toggleRule(list, id); preview(); };
  const move = (id: string, dir: number) => { list = moveRule(list, id, dir); preview(); };
  const del = (id: string) => { list = removeRule(list, id); preview(); };

  onMount(() => {
    // Show the working copy immediately so opening the editor never blanks existing colours.
    preview();
  });
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Color rules" on:click|stopPropagation>
    <h2>Color rules</h2>
    <p>The first enabled rule that matches a file supplies its name colour and label. Drag order = priority.</p>

    <div class="rules" data-testid="rules-list">
      {#if list.length === 0}
        <div class="empty">No rules yet — add one below.</div>
      {/if}
      {#each list as rule, i (rule.id)}
        <div class="rule" class:disabled={rule.enabled === false} data-testid="rule-row">
          <input
            type="checkbox"
            checked={rule.enabled !== false}
            title="Enable rule"
            aria-label="Enable rule"
            on:change={() => toggle(rule.id)}
          />
          <input
            type="color"
            value={rule.color ?? "#888888"}
            title="Colour"
            aria-label="Rule colour"
            on:input={(e) => setColor(rule.id, e.currentTarget.value)}
          />
          <span class="summary" title={summarize(rule.when)}>{summarize(rule.when)}</span>
          <input
            class="label-input"
            placeholder="label (optional)"
            value={rule.label ?? ""}
            aria-label="Rule label"
            on:input={(e) => setLabel(rule.id, e.currentTarget.value)}
          />
          <button class="mini" title="Move up" aria-label="Move up" disabled={i === 0} on:click={() => move(rule.id, -1)}>↑</button>
          <button class="mini" title="Move down" aria-label="Move down" disabled={i === list.length - 1} on:click={() => move(rule.id, 1)}>↓</button>
          <button class="mini danger" title="Delete rule" aria-label="Delete rule" on:click={() => del(rule.id)}>✕</button>
        </div>
      {/each}
    </div>

    <div class="add" data-testid="add-rule">
      <select bind:value={kind} aria-label="Condition kind">
        <option value="ext">Extension</option>
        <option value="glob">Name (glob)</option>
        <option value="size">Size</option>
        <option value="olderThan">Older than</option>
        <option value="newerThan">Newer than</option>
        <option value="isDir">Is folder</option>
      </select>

      {#if kind === "ext"}
        <input class="grow" placeholder="ts, md, png" bind:value={exts} aria-label="Extensions" />
      {:else if kind === "glob"}
        <input class="grow" placeholder="*.min.js" bind:value={glob} aria-label="Glob pattern" />
      {:else if kind === "size"}
        <input class="num" placeholder="min bytes" bind:value={sizeMin} aria-label="Min bytes" />
        <input class="num" placeholder="max bytes" bind:value={sizeMax} aria-label="Max bytes" />
      {:else if kind === "olderThan" || kind === "newerThan"}
        <input class="num" bind:value={days} aria-label="Days" /> <span class="unit">days</span>
      {:else if kind === "isDir"}
        <label class="chk"><input type="checkbox" bind:checked={isDirValue} /> folder</label>
      {/if}

      <input type="color" bind:value={newColor} title="Colour" aria-label="New rule colour" />
      <input class="label-input" placeholder="label" bind:value={newLabel} aria-label="New rule label" />
      <button class="btn" data-testid="add-btn" on:click={add}>Add</button>
    </div>

    <div class="actions">
      <button class="btn" on:click={() => dispatch("cancel")}>Cancel</button>
      <button class="btn primary" data-testid="done-btn" on:click={() => dispatch("save", list)}>Done</button>
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
    width: 620px;
    max-width: 94vw;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 8px; }
  p { color: var(--text-dim); font-size: 12.5px; margin-bottom: 12px; line-height: 1.5; }
  .rules { max-height: 46vh; overflow-y: auto; display: flex; flex-direction: column; gap: 6px; }
  .empty { color: var(--text-dim); font-size: 12.5px; padding: 10px 2px; }
  .rule {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-alt);
  }
  .rule.disabled { opacity: 0.5; }
  .summary { flex: 1 1 auto; font-size: 12.5px; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .label-input { width: 120px; height: 26px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); }
  .add { display: flex; align-items: center; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .add .grow { flex: 1 1 120px; }
  .add input:not([type="color"]):not(.label-input), .add select { height: 30px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); }
  .num { width: 90px; }
  .unit, .chk { font-size: 12.5px; color: var(--text-dim); }
  input[type="color"] { width: 32px; height: 30px; padding: 0; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); }
  .mini {
    width: 26px; height: 26px; display: grid; place-items: center;
    border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--surface); color: var(--text);
  }
  .mini:disabled { opacity: 0.35; }
  .mini.danger { color: var(--text); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
  .btn {
    height: 32px; padding: 0 16px;
    border: 1px solid var(--border-strong); border-radius: var(--radius);
    background: var(--surface-alt); color: var(--text);
  }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover { background: var(--accent-hover); }
</style>
