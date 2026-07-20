<script lang="ts">
  /**
   * Watched-folder rules editor (CPE-795, epic CPE-734). Define trigger (CPE-774 `Condition`) → action
   * pipeline (move / copy / tag / rename) rules, reorder / enable / delete them, and **dry-run** a sample
   * filename through the planner (`planForEntry`, CPE-793) to preview what the first matching rule would
   * do — before any live watcher runs them. A thin CRUD over the tested `watchRules` store; rules persist
   * via settings (App owns the store). The live executor + activity log/undo are a separate follow-up
   * (CPE-794 tail).
   */
  import { createEventDispatcher } from "svelte";
  import { addRule, removeRule, setRuleEnabled, moveRule, planForEntry, type WatchRule, type Action } from "../watchRules";
  import type { Condition } from "../colorRules";
  import type { DirEntry } from "../types";

  export let rules: WatchRule[] = [];

  const dispatch = createEventDispatcher<{ save: WatchRule[]; cancel: void }>();

  let list: WatchRule[] = rules.map((r) => ({ ...r }));

  // New-rule form.
  let ruleName = "";
  let kind: Condition["kind"] = "ext";
  let exts = "";
  let glob = "";
  let sizeMin = "";
  let sizeMax = "";
  let days = "7";
  let isDirValue = false;
  // Pending action pipeline for the new rule.
  let actKind: Action["kind"] = "move";
  let actValue = "";
  let pending: Action[] = [];

  // Dry-run preview.
  let sampleName = "invoice.pdf";
  $: preview = planForEntry(
    { name: sampleName, path: "/sample/" + sampleName, is_dir: false, size: 1000, modified: Date.now() } as DirEntry,
    list,
    Date.now(),
  );

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
        return min === undefined && max === undefined ? null : { kind: "size", min, max };
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

  function buildAction(): Action | null {
    const v = actValue.trim();
    switch (actKind) {
      case "move":
      case "copy":
        return v ? { kind: actKind, dest: v } : null;
      case "tag":
        return v ? { kind: "tag", tag: v } : null;
      case "rename":
        return v ? { kind: "rename", template: v } : null;
    }
  }

  function addAction() {
    const a = buildAction();
    if (a) { pending = [...pending, a]; actValue = ""; }
  }

  function addTheRule() {
    const cond = buildCondition();
    if (!cond || !ruleName.trim() || pending.length === 0) return;
    list = addRule(list, ruleName.trim(), cond, pending);
    ruleName = ""; exts = glob = sizeMin = sizeMax = ""; pending = [];
  }

  function condSummary(c: Condition): string {
    switch (c.kind) {
      case "ext": return `.${c.exts.join("/.")}`;
      case "glob": return c.pattern;
      case "size": return `size ${c.min ?? 0}–${c.max ?? "∞"}b`;
      case "olderThan": return `>${c.days}d old`;
      case "newerThan": return `<${c.days}d old`;
      case "isDir": return c.value ? "folders" : "files";
    }
  }
  function actSummary(a: Action): string {
    switch (a.kind) {
      case "move": return `move → ${a.dest}`;
      case "copy": return `copy → ${a.dest}`;
      case "tag": return `tag ${a.tag}`;
      case "rename": return `rename ${a.template}`;
    }
  }

  const toggle = (id: string, on: boolean) => { list = setRuleEnabled(list, id, on); };
  const move = (id: string, dir: number) => { list = moveRule(list, id, dir); };
  const del = (id: string) => { list = removeRule(list, id); };
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Watch rules" on:click|stopPropagation>
    <h2>Watched-folder rules</h2>

    <div class="rules" data-testid="watch-rules-list">
      {#if list.length === 0}<div class="empty">No rules yet.</div>{/if}
      {#each list as rule, i (rule.id)}
        <div class="rule" class:disabled={rule.enabled === false} data-testid="watch-rule-row">
          <input type="checkbox" checked={rule.enabled !== false} aria-label="Enable rule" on:change={(e) => toggle(rule.id, e.currentTarget.checked)} />
          <span class="rname">{rule.name}</span>
          <span class="cond">when {condSummary(rule.when)}</span>
          <span class="acts">{rule.actions.map(actSummary).join(", ")}</span>
          <button class="mini" aria-label="Move up" disabled={i === 0} on:click={() => move(rule.id, -1)}>↑</button>
          <button class="mini" aria-label="Move down" disabled={i === list.length - 1} on:click={() => move(rule.id, 1)}>↓</button>
          <button class="mini danger" aria-label="Delete rule" on:click={() => del(rule.id)}>✕</button>
        </div>
      {/each}
    </div>

    <div class="builder" data-testid="add-watch-rule">
      <input class="rule-name" placeholder="Rule name" bind:value={ruleName} aria-label="Rule name" />
      <div class="brow">
        <span class="lbl">When</span>
        <select bind:value={kind} aria-label="Condition kind">
          <option value="ext">extension</option>
          <option value="glob">name (glob)</option>
          <option value="size">size</option>
          <option value="olderThan">older than</option>
          <option value="newerThan">newer than</option>
          <option value="isDir">is folder</option>
        </select>
        {#if kind === "ext"}<input class="grow" placeholder="pdf, zip" bind:value={exts} aria-label="Extensions" />
        {:else if kind === "glob"}<input class="grow" placeholder="*.tmp" bind:value={glob} aria-label="Glob" />
        {:else if kind === "size"}<input class="num" placeholder="min" bind:value={sizeMin} aria-label="Min bytes" /><input class="num" placeholder="max" bind:value={sizeMax} aria-label="Max bytes" />
        {:else if kind === "olderThan" || kind === "newerThan"}<input class="num" bind:value={days} aria-label="Days" /><span class="lbl">days</span>
        {:else if kind === "isDir"}<label class="lbl"><input type="checkbox" bind:checked={isDirValue} /> folder</label>{/if}
      </div>
      <div class="brow">
        <span class="lbl">Do</span>
        <select bind:value={actKind} aria-label="Action kind">
          <option value="move">move to</option>
          <option value="copy">copy to</option>
          <option value="tag">tag</option>
          <option value="rename">rename to</option>
        </select>
        <input class="grow" placeholder={actKind === "rename" ? "{stem}-archived.{ext}" : actKind === "tag" ? "tag name" : "dest folder"} bind:value={actValue} aria-label="Action value" on:keydown={(e) => e.key === "Enter" && addAction()} />
        <button class="btn" data-testid="add-action-btn" on:click={addAction}>+ action</button>
      </div>
      {#if pending.length}
        <div class="pending" data-testid="pending-actions">
          {#each pending as a, i (i)}<span class="chip">{actSummary(a)}</span>{/each}
        </div>
      {/if}
      <button class="btn primary" data-testid="add-rule-btn" disabled={!ruleName.trim() || pending.length === 0} on:click={addTheRule}>Add rule</button>
    </div>

    <div class="preview" data-testid="dry-run">
      <span class="lbl">Dry-run</span>
      <input class="grow" bind:value={sampleName} aria-label="Sample filename" />
      <span class="result" data-testid="dry-run-result">
        {#if preview}→ <b>{preview.rule.name}</b>: {preview.actions.map((a) => a.resolved).join(", ")}{:else}no rule matches{/if}
      </span>
    </div>

    <div class="actions">
      <button class="btn" on:click={() => dispatch("cancel")}>Cancel</button>
      <button class="btn primary" data-testid="done-btn" on:click={() => dispatch("save", list)}>Done</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 680px; max-width: 95vw; background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 16px; margin-bottom: 12px; }
  .rules { max-height: 34vh; overflow-y: auto; display: flex; flex-direction: column; gap: 5px; margin-bottom: 12px; }
  .empty { color: var(--text-dim); font-size: 12.5px; padding: 8px 2px; }
  .rule { display: flex; align-items: center; gap: 8px; padding: 5px 6px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt); font-size: 12.5px; }
  .rule.disabled { opacity: 0.5; }
  .rname { font-weight: 600; flex: 0 0 auto; }
  .cond { color: var(--text-dim); flex: 0 0 auto; }
  .acts { flex: 1 1 auto; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .builder { border: 1px solid var(--border); border-radius: var(--radius); padding: 10px; display: flex; flex-direction: column; gap: 8px; }
  .brow { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .brow .grow { flex: 1 1 120px; }
  .rule-name { height: 30px; padding: 0 8px; }
  select, input:not([type="checkbox"]) { height: 30px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0; }
  .num { width: 80px; }
  .lbl { font-size: 12px; color: var(--text-dim); }
  .pending { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { flex: 0 0 auto; padding: 1px 8px; border-radius: 999px; font-size: 11px; background: var(--accent); color: #fff; white-space: nowrap; }
  .preview { display: flex; align-items: center; gap: 8px; margin-top: 12px; }
  .preview .grow { flex: 0 1 220px; height: 30px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); }
  .result { font-size: 12.5px; color: var(--text); }
  .mini { width: 24px; height: 24px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); }
  .mini:disabled { opacity: 0.35; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
  .btn { height: 30px; padding: 0 14px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
