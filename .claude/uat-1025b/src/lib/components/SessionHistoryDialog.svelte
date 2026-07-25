<script lang="ts">
  /**
   * Session-history browser + filtered export (CPE-801, epic CPE-733). Browses past Agent-Watch sessions
   * from the on-disk audit journal (CPE-800 `audit_sessions` / `audit_read`), filters their events
   * (CPE-799 `filterEvents`), optionally redacts sensitive paths, and exports the current selection to
   * JSON / CSV / Markdown. A thin render over the tested logic — the file save is delegated to App.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import {
    filterEvents,
    redactEvents,
    toJson,
    toCsv,
    toMarkdown,
    type AuditEvent,
  } from "../auditExport";
  import { formatDate } from "../datetime";

  /** Home dir for the "redact home paths" option (App supplies it). */
  export let home = "";

  const dispatch = createEventDispatcher<{
    export: { format: string; ext: string; content: string };
    cancel: void;
  }>();

  const KINDS = ["created", "modified", "removed", "renamed", "read"] as const;

  let sessions: string[] = [];
  let selected = "";
  let events: AuditEvent[] = [];
  let loading = false;
  let error = "";

  // Filters.
  let activeKinds = new Set<string>();
  let pathIncludes = "";
  let redact = false;

  $: filtered = filterEvents(events, {
    kinds: activeKinds.size ? [...activeKinds] : undefined,
    pathIncludes: pathIncludes.trim() || undefined,
  });

  $: forExport = redact && home ? redactEvents(filtered, { home }) : filtered;

  onMount(async () => {
    try {
      sessions = unwrap(await commands.auditSessions());
      if (sessions.length) select(sessions[sessions.length - 1]);
    } catch (e) {
      error = String(e);
    }
  });

  async function select(s: string) {
    selected = s;
    loading = true;
    error = "";
    try {
      events = unwrap(await commands.auditRead(s)) as AuditEvent[];
    } catch (e) {
      error = String(e);
      events = [];
    } finally {
      loading = false;
    }
  }

  function toggleKind(k: string) {
    const next = new Set(activeKinds);
    next.has(k) ? next.delete(k) : next.add(k);
    activeKinds = next;
  }

  function doExport(format: "json" | "csv" | "md") {
    const content = format === "json" ? toJson(forExport) : format === "csv" ? toCsv(forExport) : toMarkdown(forExport);
    const ext = format === "md" ? "md" : format;
    dispatch("export", { format, ext, content });
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Session history" on:click|stopPropagation>
    <h2>Session history</h2>

    <div class="body">
      <div class="sessions" data-testid="session-list">
        {#if sessions.length === 0}
          <div class="empty">No recorded sessions yet.</div>
        {/if}
        {#each sessions as s (s)}
          <button class="session" class:active={s === selected} data-testid="session-item" on:click={() => select(s)}>
            {s}
          </button>
        {/each}
      </div>

      <div class="events">
        <div class="filters">
          {#each KINDS as k}
            <label class="chip"><input type="checkbox" checked={activeKinds.has(k)} on:change={() => toggleKind(k)} /> {k}</label>
          {/each}
          <input class="path-filter" placeholder="path contains…" bind:value={pathIncludes} aria-label="Path filter" />
          <label class="chip"><input type="checkbox" bind:checked={redact} /> redact home</label>
        </div>

        {#if error}
          <div class="err">{error}</div>
        {:else if loading}
          <div class="empty">Loading…</div>
        {:else}
          <div class="event-list" data-testid="event-list">
            {#each filtered as e (e.ts + e.path + e.kind)}
              <div class="event" data-testid="event-row">
                <span class="ts">{formatDate(e.ts)}</span>
                <span class="kind kind-{e.kind}">{e.kind}</span>
                <span class="path" title={e.path}>{e.path}</span>
              </div>
            {/each}
            {#if filtered.length === 0}
              <div class="empty">No events match.</div>
            {/if}
          </div>
          <div class="count" data-testid="event-count">{filtered.length} event{filtered.length === 1 ? "" : "s"}</div>
        {/if}
      </div>
    </div>

    <div class="actions">
      <div class="exports">
        <span class="lbl">Export:</span>
        <button class="btn" data-testid="export-json" disabled={forExport.length === 0} on:click={() => doExport("json")}>JSON</button>
        <button class="btn" data-testid="export-csv" disabled={forExport.length === 0} on:click={() => doExport("csv")}>CSV</button>
        <button class="btn" data-testid="export-md" disabled={forExport.length === 0} on:click={() => doExport("md")}>Markdown</button>
      </div>
      <button class="btn primary" on:click={() => dispatch("cancel")}>Close</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 760px; max-width: 95vw; background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 16px; margin-bottom: 12px; }
  .body { display: flex; gap: 12px; height: 52vh; }
  .sessions { flex: 0 0 190px; overflow-y: auto; display: flex; flex-direction: column; gap: 4px; border: 1px solid var(--border); border-radius: var(--radius); padding: 6px; }
  .session { text-align: left; padding: 6px 8px; border: 1px solid transparent; border-radius: var(--radius); background: var(--surface-alt); color: var(--text); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .session.active { border-color: var(--accent); }
  .events { flex: 1 1 auto; display: flex; flex-direction: column; min-width: 0; }
  .filters { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-bottom: 8px; }
  .chip { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--text-dim); white-space: nowrap; }
  .path-filter { flex: 1 1 120px; height: 28px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); }
  .event-list { flex: 1 1 auto; overflow-y: auto; border: 1px solid var(--border); border-radius: var(--radius); }
  .event { display: flex; align-items: baseline; gap: 8px; padding: 3px 8px; font-size: 12px; border-bottom: 1px solid var(--border); }
  .ts { flex: 0 0 auto; color: var(--text-dim); font-variant-numeric: tabular-nums; }
  .kind { flex: 0 0 auto; text-transform: uppercase; font-size: 10px; letter-spacing: 0.03em; }
  .path { flex: 1 1 auto; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .count { padding: 6px 2px 0; font-size: 11.5px; color: var(--text-dim); }
  .empty, .err { padding: 10px; color: var(--text-dim); font-size: 12.5px; }
  .err { color: var(--danger, #c0392b); }
  .actions { display: flex; justify-content: space-between; align-items: center; margin-top: 14px; }
  .exports { display: flex; align-items: center; gap: 6px; }
  .lbl { font-size: 12px; color: var(--text-dim); }
  .btn { height: 30px; padding: 0 14px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
