<script lang="ts">
  /**
   * Rules-based auto-organize (CPE-1142, epic CPE-979 "rules-based" slice). The preview/approve UI over
   * the `organize_plan` / `organize_apply` commands: pick a rule, see the proposed moves grouped by
   * destination subfolder, then Apply. Nothing on disk moves until Apply is clicked, and Apply always
   * checkpoints the folder first (`cpe_server::organize_apply::organize_apply`), so the whole reorg is a
   * single one-click Undo via Checkpoint & rollback — this dialog surfaces that checkpoint and offers to
   * open the rollback dialog on it directly.
   */
  import { createEventDispatcher } from "svelte";
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import type { OrganizeRule, MoveProposal, OrganizeApplyOutcome } from "../bindings.gen";
  import { t } from "../i18n";
  import Icon from "./Icon.svelte";

  /** Folder to organize (the current folder — fixed, not a free-text field, like Find Duplicates). */
  export let path = "";

  const dispatch = createEventDispatcher<{ applied: void; undo: void; help: void; cancel: void }>();

  const RULES: { value: OrganizeRule; labelKey: string }[] = [
    { value: "by_kind", labelKey: "org.ruleByKind" },
    { value: "by_extension", labelKey: "org.ruleByExtension" },
    { value: "by_modified_year", labelKey: "org.ruleByYear" },
    { value: "by_size_bucket", labelKey: "org.ruleBySize" },
  ];
  let rule: OrganizeRule = "by_kind";

  let plan: MoveProposal[] = [];
  let loading = false;
  let error = "";
  let applying = false;
  let outcome: OrganizeApplyOutcome | null = null;

  interface Group { subdir: string; items: MoveProposal[] }
  $: groups = groupBy(plan);
  function groupBy(items: MoveProposal[]): Group[] {
    const map = new Map<string, MoveProposal[]>();
    for (const it of items) {
      const arr = map.get(it.target_subdir);
      if (arr) arr.push(it);
      else map.set(it.target_subdir, [it]);
    }
    return [...map.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([subdir, groupItems]) => ({ subdir, items: groupItems }));
  }

  let loadGen = 0; // supersede a stale preview when the rule changes again before it lands
  async function loadPlan() {
    if (!path.trim()) { plan = []; return; }
    const gen = ++loadGen;
    loading = true;
    error = "";
    outcome = null;
    try {
      const result = unwrap(await commands.organizePlan(path, rule));
      if (gen === loadGen) plan = result;
    } catch (e) {
      if (gen === loadGen) { error = String(e); plan = []; }
    } finally {
      if (gen === loadGen) loading = false;
    }
  }

  // Debounce rule switches so rapidly clicking through the rule picker fires one invoke, not four.
  let debounceHandle: ReturnType<typeof setTimeout> | undefined;
  function scheduleLoad() {
    if (debounceHandle) clearTimeout(debounceHandle);
    debounceHandle = setTimeout(loadPlan, 120);
  }
  $: rule, path, scheduleLoad();

  async function apply() {
    if (plan.length === 0 || applying || loading) return;
    applying = true;
    error = "";
    try {
      outcome = unwrap(await commands.organizeApply(path, rule));
      dispatch("applied");
    } catch (e) {
      error = String(e);
    } finally {
      applying = false;
    }
  }

  $: movedCount = outcome ? outcome.results.filter((r) => r.ok).length : 0;
  $: skippedCount = outcome ? outcome.results.filter((r) => !r.ok).length : 0;
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={$t("org.title")} on:click|stopPropagation>
    <div class="head-row">
      <h2>{$t("org.title")}</h2>
      <button class="docs" title="Open documentation" aria-label="Open documentation" data-testid="help-btn"
        on:click={() => dispatch("help")}><Icon name="book" size={15} /></button>
    </div>

    <div class="rules" data-testid="rule-picker">
      {#each RULES as r (r.value)}
        <button class="rule" class:active={rule === r.value} data-testid="rule-{r.value}"
          on:click={() => (rule = r.value)}>{$t(r.labelKey)}</button>
      {/each}
    </div>

    {#if error}<div class="err" data-testid="error">{error}</div>{/if}

    {#if outcome}
      <div class="outcome" data-testid="outcome-panel">
        <p>{$t("org.result", { moved: movedCount, skipped: skippedCount })}</p>
        <p class="checkpoint-note">
          {$t("org.checkpointNote", { label: outcome.checkpoint.checkpoint.label || outcome.checkpoint.checkpoint.manifest_id.slice(0, 12) })}
        </p>
        <button class="btn" data-testid="undo-btn" on:click={() => dispatch("undo")}>{$t("org.undo")}</button>
      </div>
    {:else}
      <div class="preview" data-testid="preview">
        {#if loading && plan.length === 0}
          <div class="empty">{$t("org.loading")}</div>
        {:else if plan.length === 0}
          <div class="empty" data-testid="empty-state">{$t("org.empty")}</div>
        {:else}
          <div class="summary" data-testid="summary">
            {$t("org.willMove", { count: plan.length, groups: groups.length })}
          </div>
          <div class="groups">
            {#each groups as g (g.subdir)}
              <div class="group" data-testid="group-{g.subdir}">
                <div class="group-head">
                  <span class="pill">{g.subdir} · {g.items.length}</span>
                </div>
                <div class="group-items">
                  {#each g.items as it (it.name)}
                    <div class="item" title={it.name}>{it.name}</div>
                  {/each}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <div class="actions">
      <button class="btn" data-testid="cancel-btn" on:click={() => dispatch("cancel")}>{$t("common.cancel")}</button>
      {#if !outcome}
        <button class="btn primary" data-testid="apply-btn" disabled={plan.length === 0 || loading || applying}
          on:click={apply}>{applying ? $t("org.applying") : $t("org.apply")}</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 620px; max-width: 95vw; max-height: 85vh; overflow: auto; background: var(--surface); border: 1px solid var(--dialog-border); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  .head-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-bottom: 10px; }
  h2 { font-size: 16px; }
  .docs { display: grid; place-items: center; height: 26px; width: 26px; padding: 0; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .rules { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 12px; }
  .rule { height: 28px; padding: 0 12px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); color: var(--text); font-size: 12px; white-space: nowrap; flex: 0 0 auto; }
  .rule.active { background: var(--accent); border-color: var(--accent); color: #fff; }
  .err { color: var(--danger); font-size: 12.5px; margin-bottom: 8px; }
  .preview { border: 1px solid var(--border); border-radius: var(--radius); padding: 10px; min-height: 120px; max-height: 45vh; overflow: auto; margin-bottom: 12px; }
  .empty { color: var(--text-dim); font-size: 12.5px; padding: 8px 2px; }
  .summary { font-size: 12.5px; color: var(--text-dim); margin-bottom: 10px; }
  .groups { display: flex; flex-direction: column; gap: 10px; }
  .group-head { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 4px; }
  .pill { display: inline-flex; align-items: center; white-space: nowrap; flex: 0 0 auto; max-width: 100%; overflow: hidden; text-overflow: ellipsis; height: 22px; padding: 0 10px; border-radius: 999px; background: var(--surface-alt); border: 1px solid var(--border-strong); font-size: 11.5px; color: var(--text); }
  .group-items { display: flex; flex-direction: column; gap: 2px; padding-left: 4px; }
  .item { font-size: 12px; color: var(--text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-family: ui-monospace, "Cascadia Code", "Consolas", monospace; }
  .outcome { border: 1px solid var(--border); border-radius: var(--radius); padding: 12px; margin-bottom: 12px; }
  .outcome p { font-size: 12.5px; color: var(--text); margin-bottom: 6px; }
  .checkpoint-note { color: var(--text-dim); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
  .btn { height: 30px; padding: 0 14px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn:disabled { opacity: 0.5; }
</style>
