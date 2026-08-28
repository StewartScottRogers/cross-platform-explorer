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
  import { displaySafeName } from "../filename";

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
                    <div class="item" title={displaySafeName(it.name)}>{displaySafeName(it.name)}</div>
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
  /*
   * CPE-1968 — .preview's height MUST NOT depend on the plan. This is the fix for a silently
   * swallowed click, and the reasoning is recorded here because the shape of the CSS is the fix.
   *
   * THE DEFECT. This box used to be `min-height: 120px; max-height: 45vh`, i.e. its height was a
   * function of its CONTENT. While the first `organize_plan` is in flight the box sits at its 120px
   * floor; when the plan lands (120ms later — `scheduleLoad`'s debounce, in the script above) it
   * grows to as much as 45vh. `.backdrop` centres the dialog VERTICALLY (`place-items: center`, the
   * app-wide convention — 28 components declare it), so the dialog's growth is split evenly above
   * and below: at the 1000x700 window gui-smoke uses, 45vh - 120px = 195px of growth moved the
   * `.rules` row — and the four 28px rule pills in it — UP by ~98px, an eighth of a second after the
   * dialog appeared. A pointer resting on "By extension" then found itself inside `.preview`, whose
   * ancestor `.dialog` carries `on:click|stopPropagation`, so the click was swallowed in SILENCE: no
   * rule change, no error, no feedback. Same jump on every rule switch, since a by_kind plan and a
   * by_extension plan are different heights. Diagnosed in CPE-1965 (3 of 69 shard-4 CI jobs, 4.3%).
   *
   * WHY THIS FIX AND NOT THE OTHER TWO (decision made by the Foreman on CPE-1968; recorded here so
   * it is not silently re-litigated by the next person who finds this box roomy):
   *   - "Stop centring the backdrop" (`place-items: start center` + a top offset) fixes it completely
   *     and is the cleanest in isolation — but 28 components share the centred-backdrop rule.
   *     Changing this one dialog makes it visibly inconsistent with the other 27, and changing all 28
   *     is a different and much larger ticket. Not this one.
   *   - "Freeze the measured height while `loading`" keeps a short plan's box small, but needs JS
   *     measurement AND has a first-load case with no previous height to hold — a special case on
   *     the exact code path that is broken today.
   *   - A single stable height removes the jump on open AND on every rule switch, with no JS, no
   *     measurement and no first-load exception. Its cost is a mostly-empty box for a two-file plan.
   *     That cost is PREDICTABLE, and PURPOSE.md's tiebreaker is fast / small / predictable — a
   *     stable dialog that is sometimes roomy beats one that moves under the pointer.
   *
   * THE INVARIANT, which is what `OrganizeDialog.test.ts` asserts: the height must not depend on the
   * PLAN. Depending on the viewport is fine — the viewport does not change while a plan loads. So a
   * `clamp()` of vh between two px bounds is allowed; a `min-height`/`max-height` pair is not,
   * because that is content-driven by definition. Do not reintroduce one.
   *
   * WHY 200/40vh/340, measured rather than picked. Content height derives from the declarations
   * below: 22px of padding+border, `.summary` ~15px + 10px margin, then per group `.pill` 22px +
   * 4px margin, per item ~16px, 2px between items, 10px between groups. That puts a two-file /
   * two-group plan at ~141px and a 4-group / 20-file plan (an ordinary Downloads folder) at ~533px,
   * i.e. real plans are usually TALLER than any box we would want in a 620px dialog — this is a
   * scroll viewport, and the useful question is only how many rows it shows before scrolling. 40vh
   * is 280px at gui-smoke's 700px window (~14 file rows) and the clamp stops it being absurd at the
   * extremes: 340px caps it on a 1080px screen (45vh there would be 486px of mostly-empty box for a
   * two-file plan) and 200px floors it on a short window. (`app.css` sets
   * `* { box-sizing: border-box }` globally, so that height INCLUDES the 10px padding and the 1px
   * borders — hence the 22px term above.)
   *
   * NOTE WHICH TERM IS THE KNOB. At 700px, 40vh = 280px, so the 200px FLOOR never binds — it engages
   * only below a 500px window. Anyone tuning this box is tuning the 40vh, not the clamp bounds, and
   * 40vh should stay where it is: at 280px a 5-group plan already shows only 2 of its 5 groups, so
   * shrinking it hurts the common case to flatter the rare one.
   *
   * THE ONE PLACE THIS COSTS SOMETHING, quantified rather than waved at. Dialog height is 160 + this
   * box. At the 200px floor that is 360px, which needs a window of >= ~424px to stay inside
   * `max-height: 85vh`; the old content-driven box on a two-file plan was ~141px, i.e. a 301px dialog
   * needing only >= ~354px. So between roughly 354px and 424px of window height the dialog now
   * scrolls internally where it previously did not. Part of that band IS reachable, so do not read
   * this as theoretical: `src-tauri/src/lib.rs` sets `.min_inner_size(600.0, 400.0)`, i.e. the window
   * can legitimately be 400px tall, which puts 400-424px inside the band. Left as is deliberately —
   * `.dialog`'s own `overflow: auto` degrades it to a scrollbar, the app opens at 700px, and a
   * scrollbar on a deliberately-shrunk window is a far better outcome than a dialog that moves under
   * the pointer at every size. But it is a real regression in that band, recorded so the next person
   * meets it here rather than by surprise.
   *
   * NOT FIXED HERE — TWO REMAINING HEIGHT CHANGES, so this list is not mistaken for exhaustive:
   *
   *   1. `.err` renders ABOVE this box, so a rule whose plan ERRORS moves the pills relative to one
   *      that succeeds. Note this IS the load path and IS the same t=120ms moment — the earlier
   *      draft of this comment wrongly dismissed it as "a failure path". What makes it acceptable is
   *      MAGNITUDE: `.err` is ~15px of text plus its 8px margin, so the pills move ~11.5px, against
   *      a pill that is 28px tall. A pointer aimed at a pill's centre has ~14px of slack in each
   *      direction, so it stays inside the pill it was aiming at. The 195px growth this ticket fixed
   *      did not.
   *   2. `{#if outcome}` REPLACES this box with `.outcome` entirely, so the dialog shrinks and
   *      re-centres when `organizeApply` resolves. Not this ticket's shape — it is user-initiated
   *      (it follows the user's own Apply click) rather than arriving unbidden after mount, and
   *      `applying` disables Apply for the duration while the button is removed afterwards, so the
   *      obvious double-click is inert. Recorded rather than fixed, and honestly: where a stray
   *      post-Apply click lands has NOT been measured.
   */
  .preview { border: 1px solid var(--border); border-radius: var(--radius); padding: 10px; height: clamp(200px, 40vh, 340px); overflow: auto; margin-bottom: 12px; }
  /*
   * CPE-1968: CENTRED, and only because `.preview` is now a fixed 280px. `.empty` is the sole child
   * of `.preview` in exactly two states — "Loading preview…" and "nothing to organize" — and in both
   * it is ONE LINE. At the old 120px floor, flush top-left read fine. At 280px a single line pinned
   * to the corner of a large bordered box is the one thing that reads as unfinished rather than
   * roomy, so it is centred in the space the fix created.
   *
   * This must NOT extend to the plan-rendered case, where top alignment is correct for a list — and
   * it structurally cannot: that branch renders `.summary` + `.groups` and never mounts `.empty` at
   * all. `height: 100%` resolves against `.preview`'s definite height, which is the other thing the
   * fix made possible; before it, there was no definite height for a percentage to resolve against.
   */
  .empty { color: var(--text-dim); font-size: 12.5px; padding: 8px 2px; display: grid; place-items: center; height: 100%; }
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
