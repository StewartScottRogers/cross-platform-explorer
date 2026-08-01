<script lang="ts">
  /**
   * Scheduled snapshots — the Settings section for the background snapshot scheduler (CPE-1199, epic
   * CPE-735). Lists the per-folder rules and lets the user add/remove a watched folder, set its capture
   * interval + retention (hourly/daily/weekly/monthly kept), and enable/disable it — all through the
   * typed `snapshotSchedule*` commands (CPE-1198). The actual capture+prune runs on a background timer
   * in the Rust host; this is purely the opt-in configuration surface.
   *
   * Conventions honoured:
   *  - Off means off — a folder is only ever captured once the user adds an *enabled* rule here; there
   *    is no launch-time consent modal ([[avoid-modal-permission-popups]]).
   *  - Inline instant controls ([[prefer-inline-instant-controls]]) — every per-rule control saves on
   *    change (no separate "apply" step); the path field offers a native Browse picker
   *    ([[path-inputs-need-picker]]).
   */
  import { onMount } from "svelte";
  import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
  import { commands, type ScheduleRule, type RetentionPolicy } from "../bindings.gen";

  type Unit = "minutes" | "hours" | "days";
  const UNIT_S: Record<Unit, number> = { minutes: 60, hours: 3600, days: 86400 };
  const DEFAULT_RETENTION: RetentionPolicy = { hourly: 24, daily: 7, weekly: 4, monthly: 12 };
  const RETENTION_KEYS: (keyof RetentionPolicy)[] = ["hourly", "daily", "weekly", "monthly"];

  let rules: ScheduleRule[] = [];
  let error = "";

  // Add-a-rule builder state.
  let newPath = "";
  let newValue = 1;
  let newUnit: Unit = "days";

  /** Split an interval in seconds into the largest whole unit that divides it, for a friendly display. */
  function toParts(intervalS: number): { value: number; unit: Unit } {
    if (intervalS % UNIT_S.days === 0) return { value: intervalS / UNIT_S.days, unit: "days" };
    if (intervalS % UNIT_S.hours === 0) return { value: intervalS / UNIT_S.hours, unit: "hours" };
    return { value: Math.max(1, Math.round(intervalS / UNIT_S.minutes)), unit: "minutes" };
  }

  async function load() {
    const res = await commands.snapshotScheduleList();
    if (res.status === "ok") {
      rules = res.data;
      error = "";
    } else {
      error = res.error;
    }
  }

  onMount(load);

  /** Persist a rule (insert or replace by root) then refresh the list so the UI is authoritative. */
  async function save(rule: ScheduleRule) {
    const res = await commands.snapshotScheduleSet(rule);
    if (res.status === "error") {
      error = res.error;
      return;
    }
    await load();
  }

  async function browse() {
    let dest: string | string[] | null;
    try {
      dest = await openFolderDialog({
        directory: true,
        multiple: false,
        defaultPath: newPath || undefined,
        title: "Choose a folder to snapshot on a schedule",
      });
    } catch {
      return; // dialog unavailable / errored — no-op
    }
    if (typeof dest === "string") newPath = dest;
  }

  async function add() {
    const root = newPath.trim();
    if (!root) return;
    const intervalS = Math.max(1, Math.round(newValue)) * UNIT_S[newUnit];
    await save({ root, interval_s: intervalS, retention: { ...DEFAULT_RETENTION }, enabled: true });
    newPath = "";
    newValue = 1;
    newUnit = "days";
  }

  async function remove(root: string) {
    const res = await commands.snapshotScheduleRemove(root);
    if (res.status === "error") {
      error = res.error;
      return;
    }
    await load();
  }

  function setEnabled(rule: ScheduleRule, on: boolean) {
    save({ ...rule, enabled: on });
  }

  function setInterval(rule: ScheduleRule, value: number, unit: Unit) {
    const intervalS = Math.max(1, Math.round(value)) * UNIT_S[unit];
    save({ ...rule, interval_s: intervalS });
  }

  /** Re-save `rule` when its interval unit dropdown changes (the cast lives here, not in the markup). */
  function onUnitChange(rule: ScheduleRule, value: number, unit: string) {
    setInterval(rule, value, unit as Unit);
  }

  function setRetention(rule: ScheduleRule, key: keyof RetentionPolicy, value: number) {
    const retention = { ...rule.retention, [key]: Math.max(0, Math.round(value)) };
    save({ ...rule, retention });
  }
</script>

<div class="scheduled-snapshots" data-testid="scheduled-snapshots">
  {#if error}
    <div class="err" data-testid="schedule-error">{error}</div>
  {/if}

  <div class="rules" data-testid="schedule-rules">
    {#if rules.length === 0}
      <div class="empty" data-testid="schedule-empty">
        No scheduled folders yet — add one below. Nothing is captured until you do.
      </div>
    {/if}
    {#each rules as rule (rule.root)}
      {@const parts = toParts(rule.interval_s)}
      <div class="rule" data-testid="schedule-rule">
        <div class="top">
          <label class="chk" title="Enable or pause this folder's scheduled snapshots">
            <input
              type="checkbox"
              data-testid="schedule-enabled"
              checked={rule.enabled}
              on:change={(e) => setEnabled(rule, e.currentTarget.checked)}
            />
            {rule.enabled ? "on" : "paused"}
          </label>
          <span class="path" title={rule.root}>{rule.root}</span>
          <button
            class="mini"
            aria-label="Remove scheduled folder"
            data-testid="schedule-remove"
            on:click={() => remove(rule.root)}>✕</button
          >
        </div>
        <div class="controls">
          <span class="lbl">Every</span>
          <input
            class="num"
            type="number"
            min="1"
            data-testid="schedule-rule-interval"
            value={parts.value}
            on:change={(e) => setInterval(rule, e.currentTarget.valueAsNumber, parts.unit)}
          />
          <select
            class="unit"
            data-testid="schedule-rule-unit"
            value={parts.unit}
            on:change={(e) => onUnitChange(rule, parts.value, e.currentTarget.value)}
          >
            <option value="minutes">minutes</option>
            <option value="hours">hours</option>
            <option value="days">days</option>
          </select>
          <span class="lbl">· keep</span>
          {#each RETENTION_KEYS as key}
            <label class="keep">
              <input
                class="num"
                type="number"
                min="0"
                data-testid={`schedule-keep-${key}`}
                value={rule.retention[key]}
                on:change={(e) => setRetention(rule, key, e.currentTarget.valueAsNumber)}
              />
              {key}
            </label>
          {/each}
        </div>
      </div>
    {/each}
  </div>

  <div class="builder" data-testid="schedule-add">
    <input
      class="grow"
      placeholder="Folder to snapshot"
      aria-label="Folder to snapshot"
      data-testid="schedule-path"
      bind:value={newPath}
    />
    <button class="btn" data-testid="schedule-browse" on:click={browse}>Browse…</button>
    <span class="lbl">every</span>
    <input class="num" type="number" min="1" aria-label="Interval" data-testid="schedule-interval" bind:value={newValue} />
    <select class="unit" aria-label="Interval unit" data-testid="schedule-unit" bind:value={newUnit}>
      <option value="minutes">minutes</option>
      <option value="hours">hours</option>
      <option value="days">days</option>
    </select>
    <button class="btn primary" data-testid="schedule-add-btn" disabled={!newPath.trim()} on:click={add}>Add</button>
  </div>
</div>

<style>
  .scheduled-snapshots { display: flex; flex-direction: column; gap: 8px; }
  .rules { display: flex; flex-direction: column; gap: 6px; max-height: 34vh; overflow-y: auto; }
  .empty { color: var(--text-dim); font-size: 12px; padding: 4px 2px; }
  .rule {
    display: flex; flex-direction: column; gap: 6px;
    padding: 8px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt);
  }
  .top { display: flex; align-items: center; gap: 8px; }
  .path {
    flex: 1 1 auto; min-width: 0; font-size: 11.5px; color: var(--text-dim);
    font-family: ui-monospace, monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .controls { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 8px; }
  .lbl { font-size: 12px; color: var(--text-dim); }
  .keep { display: inline-flex; align-items: center; gap: 3px; font-size: 11.5px; color: var(--text-dim); white-space: nowrap; }
  .chk { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--text-dim); flex: 0 0 auto; }
  .builder { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; margin-top: 2px; }
  .builder .grow { flex: 1 1 140px; }
  input:not([type="checkbox"]), select {
    height: 28px; padding: 0 6px; font: inherit; color: var(--text);
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0;
  }
  .num { width: 52px; text-align: right; }
  .mini { width: 22px; height: 22px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); flex: 0 0 auto; }
  .btn { height: 28px; padding: 0 12px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); font-size: 12px; }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .err { padding: 6px 8px; color: #c0392b; font-size: 12px; }
</style>
