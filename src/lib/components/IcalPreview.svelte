<script lang="ts">
  // .ics iCalendar preview (CPE-1435, epic CPE-1433 "Structured previews"): wires the CPE-1435
  // `ical_preview` backend command into the preview pane. A read-only VIEWER — it shows each
  // VEVENT/VTODO/VJOURNAL as an event card (summary + when/where/who + a readable recurrence note). It
  // never launches a calendar app and never reaches the network. Self-contained like EmailPreview.svelte /
  // JwtPreview.svelte: fetches its own data from `path`, no prop-drilled callback.
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen";
  import type { IcalPreview as IcalPreviewData } from "../bindings.gen";
  import Icon from "./Icon.svelte";

  /** The `.ics` file's path. */
  export let path: string;

  let data: IcalPreviewData | null = null;
  let loading = false;
  let loadError = "";

  // Reload whenever the previewed file changes (mirrors EmailPreview's `loadedPath` guard).
  let loadedPath = "";
  $: if (path && path !== loadedPath) { loadedPath = path; void load(); }

  async function load() {
    loading = true;
    loadError = "";
    data = null;
    try {
      data = unwrap(await commands.icalPreview(path));
    } catch (e) {
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  /** A short badge for a non-VEVENT component ("Task" for VTODO, "Journal" for VJOURNAL). */
  function componentBadge(component: string): string | null {
    if (component === "VTODO") return "Task";
    if (component === "VJOURNAL") return "Journal";
    return null;
  }

  /** The "when" line: start, plus end when present. */
  function whenText(ev: IcalPreviewData["events"][number]): string {
    if (!ev.dtstart) return ev.dtend ?? "";
    return ev.dtend ? `${ev.dtstart} – ${ev.dtend}` : ev.dtstart;
  }
</script>

<div class="crypto-preview" data-testid="ical-preview">
  {#if loading}
    <p class="cp-note">Loading…</p>
  {:else if loadError}
    <p class="cp-error" data-testid="ical-load-error">Can't preview this file: {loadError}</p>
  {:else if data}
    <div class="cp-banner">
      <Icon name="calendar" size={14} />
      <span>
        Calendar viewer — read-only.{#if data.calendar_name}<span class="cal-name" data-testid="ical-calname"> {data.calendar_name}</span>{/if}{#if data.method}<span class="cal-method" data-testid="ical-method"> · {data.method}</span>{/if}
      </span>
    </div>

    {#if data.error}
      <p class="cp-error" data-testid="ical-decode-error">{data.error}</p>
    {/if}

    {#each data.events as ev}
      <div class="cp-card" data-testid="ical-event">
        <div class="cp-card-head">
          <span class="cp-summary" data-testid="ical-summary">{ev.summary ?? "(no title)"}</span>
          {#if componentBadge(ev.component)}
            <span class="cp-badge" data-testid="ical-badge">{componentBadge(ev.component)}</span>
          {/if}
          {#if ev.all_day}<span class="cp-badge cp-badge-soft" data-testid="ical-allday">All day</span>{/if}
        </div>

        <dl class="cp-rows">
          {#if whenText(ev)}
            <div><dt><Icon name="calendar" size={12} /> When</dt><dd class="wrap" data-testid="ical-when">{whenText(ev)}</dd></div>
          {/if}
          {#if ev.location}
            <div><dt><Icon name="location" size={12} /> Where</dt><dd class="wrap" data-testid="ical-where">{ev.location}</dd></div>
          {/if}
          {#if ev.organizer}
            <div><dt><Icon name="contact" size={12} /> Organizer</dt><dd class="wrap" data-testid="ical-organizer">{ev.organizer}</dd></div>
          {/if}
          {#if ev.status}
            <div><dt>Status</dt><dd class="wrap">{ev.status}</dd></div>
          {/if}
        </dl>

        {#if ev.attendees.length}
          <div class="cp-subsection">
            <div class="cp-title">{ev.attendees.length === 1 ? "1 attendee" : `${ev.attendees.length} attendees`}</div>
            <div class="pill-row" data-testid="ical-attendees">
              {#each ev.attendees as att}
                <span class="pill" title={att}><Icon name="people" size={11} /><span class="pill-name">{att}</span></span>
              {/each}
            </div>
          </div>
        {/if}

        {#if ev.recurrence}
          <p class="cp-recur" data-testid="ical-recurrence"><Icon name="refresh" size={12} /> Repeats: {ev.recurrence}</p>
        {/if}

        {#if ev.description}
          <div class="cp-subsection">
            <div class="cp-title">Description</div>
            <pre class="cp-body" data-testid="ical-description">{ev.description}</pre>
          </div>
        {/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  .crypto-preview { padding: 12px; font-size: 12px; }
  .cp-note { color: var(--text-faint); }
  .cp-error { color: var(--danger); white-space: pre-wrap; overflow-wrap: anywhere; }
  .cp-banner {
    display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-radius: var(--radius);
    background: var(--surface-alt); border: 1px solid var(--border); color: var(--text-dim);
    margin-bottom: 12px; font-size: 11.5px;
  }
  .cal-name { color: var(--text); font-weight: 600; }
  .cal-method { color: var(--text-faint); }
  .cp-card {
    border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt);
    padding: 10px 12px; margin-bottom: 12px;
  }
  .cp-card:last-child { margin-bottom: 0; }
  .cp-card-head { display: flex; align-items: baseline; flex-wrap: wrap; gap: 8px; margin-bottom: 8px; }
  .cp-summary { font-size: 13.5px; font-weight: 600; color: var(--text); overflow-wrap: anywhere; }
  .cp-badge {
    font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.03em;
    padding: 1px 7px; border-radius: 999px; background: var(--accent-soft, var(--surface));
    color: var(--accent, var(--text-dim)); border: 1px solid var(--border); white-space: nowrap; flex: 0 0 auto;
  }
  .cp-badge-soft { color: var(--text-dim); background: var(--surface); }
  .cp-title { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.03em; margin-bottom: 6px; }
  .cp-rows { display: grid; gap: 6px; margin: 0; }
  .cp-rows > div { display: flex; gap: 10px; align-items: baseline; }
  .cp-rows dt { color: var(--text-dim); width: 90px; flex: none; display: inline-flex; align-items: center; gap: 5px; }
  .cp-rows dd { flex: 1; margin: 0; overflow-wrap: anywhere; }
  .wrap { overflow-wrap: anywhere; }
  .cp-subsection { margin-top: 10px; }
  /* Tick-tacks rule (memory: pill rows reflow): the row wraps onto more lines and grows its height, while
     each pill keeps its text on one line and never shrinks. */
  .pill-row { display: flex; flex-wrap: wrap; gap: 6px; }
  .pill {
    display: inline-flex; align-items: center; gap: 6px; flex: 0 0 auto; max-width: 260px;
    padding: 3px 9px; border: 1px solid var(--border); border-radius: 999px;
    background: var(--surface); color: var(--text); font-size: 11px; white-space: nowrap;
  }
  .pill-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cp-recur { display: flex; align-items: center; gap: 6px; margin: 10px 0 0; color: var(--text-dim); font-size: 11.5px; }
  .cp-body {
    margin: 0; padding: 10px; border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--surface); font-family: var(--mono, ui-monospace, monospace); font-size: 11.5px;
    white-space: pre-wrap; word-break: break-word; max-height: 320px; overflow: auto;
  }
</style>
