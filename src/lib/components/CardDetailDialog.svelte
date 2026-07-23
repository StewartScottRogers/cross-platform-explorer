<script lang="ts">
  /**
   * Agent Board card-detail popup (CPE-959): clicking a ticket/epic card opens this read-only view of the
   * whole ticket — its frontmatter fields + the rendered markdown body. Loads via the typed `boardCardDetail`
   * client (routes through the busy-cursor invoke). For an epic it also offers "View tickets" (dispatches
   * `drill`, so the board can filter to that epic's tickets).
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { commands } from "../bindings.gen";
  import { renderMarkdown } from "../preview/markdown";
  import Icon from "./Icon.svelte";

  export let root: string;
  export let id: string;
  export let isEpic = false;

  const dispatch = createEventDispatcher<{ close: void; drill: string }>();

  type Detail = { id: string; location: string; fields: [string, string][]; body: string };
  let detail: Detail | null = null;
  let bodyHtml = "";
  let loading = true;
  let error = "";

  onMount(async () => {
    try {
      const d = (await commands.boardCardDetail(root, id)) as Detail | null;
      if (!d) error = `Couldn't find ${id} under Tickets/.`;
      else {
        detail = d;
        bodyHtml = d.body ? await renderMarkdown(d.body) : "";
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  });

  // `title` shows in the header; the rest of the frontmatter goes in the fields table.
  $: title = detail?.fields.find(([k]) => k === "title")?.[1] ?? id;
  $: metaFields = (detail?.fields ?? []).filter(([k]) => k !== "title");
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <header>
      <span class="cd-id">{id}</span>
      <h2 title={title}>{title}</h2>
      {#if isEpic}
        <button class="cd-drill" on:click={() => dispatch("drill", id)} title="Filter the board to this epic's tickets">View tickets →</button>
      {/if}
      <button class="x" title="Close" on:click={() => dispatch("close")}><Icon name="close" size={14} /></button>
    </header>

    {#if loading}
      <div class="cd-msg">Loading…</div>
    {:else if error}
      <div class="cd-msg err">{error}</div>
    {:else if detail}
      <div class="cd-loc"><Icon name="folder" size={13} /> {detail.location || "Tickets"}</div>
      {#if metaFields.length}
        <table class="cd-fields">
          <tbody>
            {#each metaFields as [k, v]}
              <tr><th>{k}</th><td>{v}</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if bodyHtml}
        <!-- Sanitized by renderMarkdown (marked + DOMPurify). -->
        <div class="cd-body">{@html bodyHtml}</div>
      {:else}
        <div class="cd-msg dim">No description.</div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.35); display: grid; place-items: center; z-index: 220; }
  .dialog {
    width: 720px; max-width: 94vw; max-height: 88vh; display: flex; flex-direction: column;
    background: var(--surface); color: var(--text); border: 1px solid var(--border-strong);
    border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.3); padding: 14px 18px 16px;
  }
  header { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; }
  .cd-id { font-family: ui-monospace, monospace; font-size: 12px; color: var(--accent); border: 1px solid var(--accent); border-radius: 5px; padding: 1px 7px; flex: 0 0 auto; }
  h2 { font-size: 16px; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cd-drill { height: 26px; padding: 0 10px; font-size: 12px; border: 1px solid var(--border-strong); border-radius: 6px; background: var(--surface-alt); flex: 0 0 auto; white-space: nowrap; }
  .cd-drill:hover { background: rgba(128,128,128,0.14); }
  .x { width: 28px; height: 28px; display: grid; place-items: center; color: var(--text-dim); flex: 0 0 auto; }
  .x:hover { color: var(--text); }
  .cd-loc { display: flex; align-items: center; gap: 5px; font-size: 12px; color: var(--text-dim); margin-bottom: 8px; }
  .cd-fields { width: 100%; border-collapse: collapse; margin-bottom: 12px; font-size: 12.5px; }
  .cd-fields th { text-align: left; width: 120px; color: var(--text-faint); font-weight: 600; text-transform: uppercase; letter-spacing: .04em; font-size: 11px; padding: 3px 10px 3px 0; vertical-align: top; }
  .cd-fields td { padding: 3px 0; font-family: ui-monospace, monospace; word-break: break-word; }
  .cd-body { overflow: auto; border-top: 1px solid var(--border); padding-top: 10px; line-height: 1.55; font-size: 13.5px; }
  .cd-body :global(h1), .cd-body :global(h2), .cd-body :global(h3) { font-size: 15px; margin: 12px 0 6px; }
  .cd-body :global(code) { font-family: ui-monospace, monospace; background: rgba(128,128,128,0.15); padding: 1px 5px; border-radius: 4px; font-size: 12px; }
  .cd-body :global(pre) { background: rgba(128,128,128,0.12); padding: 8px 10px; border-radius: 6px; overflow: auto; }
  .cd-body :global(ul), .cd-body :global(ol) { padding-left: 20px; }
  .cd-body :global(a) { color: var(--accent); }
  .cd-msg { color: var(--text-dim); padding: 14px 0; font-size: 13px; }
  .cd-msg.err { color: #d05656; }
  .cd-msg.dim { color: var(--text-faint); }
</style>
