<script lang="ts">
  /**
   * Agent Board card-detail popup (CPE-959/960): a read-only view of the whole ticket/epic — its
   * frontmatter fields + the rendered markdown body. Loads via the typed `boardCardDetail` client.
   *
   * Two modes: **embedded** (over the board) is a resizable, centred dialog with a corner thumb + a
   * statusbar and a pop-out button; **standalone** (`?card=` window, CPE-960) fills the OS window with no
   * backdrop / thumb / pop-out (the window frame resizes it). For an epic, "View tickets →" dispatches
   * `drill` so the board filters to that epic's tickets (embedded only — a standalone window has no board).
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { commands } from "../bindings.gen";
  import { renderMarkdown } from "../preview/markdown";
  import { lsGet, lsSet } from "../persist";
  import Icon from "./Icon.svelte";

  export let root: string;
  export let id: string;
  export let isEpic = false;
  export let standalone = false;

  const dispatch = createEventDispatcher<{ close: void; drill: string }>();

  type Detail = { id: string; location: string; fields: [string, string][]; body: string };
  let detail: Detail | null = null;
  let bodyHtml = "";
  let loading = true;
  let error = "";

  // --- Resize (embedded only): persisted + clamped to the viewport. -----------------------------------
  const SIZE_KEY = "cpe.cardDetailSize";
  const MIN_W = 380;
  const MIN_H = 300;
  let w = 720;
  let h = 560;
  if (!standalone) {
    try {
      const s = JSON.parse(lsGet(SIZE_KEY) ?? "null");
      if (s && typeof s.w === "number" && typeof s.h === "number") { w = s.w; h = s.h; }
    } catch { /* keep defaults */ }
  }
  function clampSize() {
    const vw = typeof window !== "undefined" ? window.innerWidth : 1200;
    const vh = typeof window !== "undefined" ? window.innerHeight : 900;
    w = Math.max(MIN_W, Math.min(w, vw - 32));
    h = Math.max(MIN_H, Math.min(h, vh - 32));
  }
  function startResize(e: MouseEvent) {
    e.preventDefault();
    const sx = e.clientX, sy = e.clientY, sw = w, sh = h;
    const move = (ev: MouseEvent) => { w = sw + (ev.clientX - sx); h = sh + (ev.clientY - sy); clampSize(); };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      lsSet(SIZE_KEY, JSON.stringify({ w: Math.round(w), h: Math.round(h) }));
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  onMount(async () => {
    if (!standalone) clampSize();
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

  // Pop the card out into its own OS window (embedded only), then close the embedded copy.
  async function popOut() {
    try {
      const url = `index.html?card=${encodeURIComponent(id)}&root=${encodeURIComponent(root)}`;
      const label = `card-detail-${id.replace(/[^A-Za-z0-9_-]/g, "")}-${Date.now()}`;
      new WebviewWindow(label, { url, title: id, width: Math.round(w), height: Math.round(h) });
      dispatch("close");
    } catch { /* window open failed — keep the embedded dialog */ }
  }

  $: title = detail?.fields.find(([k]) => k === "title")?.[1] ?? id;
  $: metaFields = (detail?.fields ?? []).filter(([k]) => k !== "title");
  $: bodyLines = detail?.body ? detail.body.split("\n").length : 0;
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="outer" class:embedded={!standalone} class:standalone
  on:click={(e) => { if (!standalone && e.target === e.currentTarget) dispatch("close"); }}>
  <div class="dialog" class:fill={standalone} style={standalone ? "" : `width:${w}px;height:${h}px`} role="dialog" aria-modal="true">
    <header>
      <span class="cd-id">{id}</span>
      <h2 title={title}>{title}</h2>
      {#if isEpic && !standalone}
        <button class="cd-drill" on:click={() => dispatch("drill", id)} title="Filter the board to this epic's tickets">View tickets →</button>
      {/if}
      {#if !standalone}
        <button class="cd-icon" title="Open in its own window" aria-label="Pop out to its own window" on:click={popOut}>⧉</button>
      {/if}
      <button class="cd-icon" title="Close" aria-label="Close" on:click={() => dispatch("close")}><Icon name="close" size={14} /></button>
    </header>

    <div class="cd-content">
      {#if loading}
        <div class="cd-msg">Loading…</div>
      {:else if error}
        <div class="cd-msg err">{error}</div>
      {:else if detail}
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

    <footer class="cd-statusbar">
      <span class="sb-loc"><Icon name="folder" size={12} /> {detail?.location || "Tickets"}</span>
      <span class="sb-meta">{metaFields.length} field{metaFields.length === 1 ? "" : "s"} · {bodyLines} line{bodyLines === 1 ? "" : "s"}</span>
      {#if !standalone}
        <!-- svelte-ignore a11y-no-static-element-interactions -->
        <div class="cd-grip" title="Drag to resize" on:mousedown={startResize}></div>
      {/if}
    </footer>
  </div>
</div>

<style>
  .outer.embedded { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.35); display: grid; place-items: center; z-index: 220; }
  .outer.standalone { position: fixed; inset: 0; background: var(--surface); }
  .dialog {
    display: flex; flex-direction: column; position: relative;
    background: var(--surface); color: var(--text); border: 1px solid var(--border-strong);
    border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.3); padding: 14px 18px 0;
    max-width: 96vw; max-height: 94vh;
  }
  .dialog.fill { width: 100%; height: 100%; border: none; border-radius: 0; box-shadow: none; max-width: none; max-height: none; }
  header { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; flex: 0 0 auto; }
  .cd-id { font-family: ui-monospace, monospace; font-size: 12px; color: var(--accent); border: 1px solid var(--accent); border-radius: 5px; padding: 1px 7px; flex: 0 0 auto; }
  h2 { font-size: 16px; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cd-drill { height: 26px; padding: 0 10px; font-size: 12px; border: 1px solid var(--border-strong); border-radius: 6px; background: var(--surface-alt); flex: 0 0 auto; white-space: nowrap; }
  .cd-drill:hover { background: rgba(128,128,128,0.14); }
  .cd-icon { width: 28px; height: 28px; display: grid; place-items: center; color: var(--text-dim); flex: 0 0 auto; font-size: 14px; }
  .cd-icon:hover { color: var(--text); }
  .cd-content { flex: 1 1 auto; overflow: auto; min-height: 0; }
  .cd-fields { width: 100%; border-collapse: collapse; margin-bottom: 12px; font-size: 12.5px; }
  .cd-fields th { text-align: left; width: 120px; color: var(--text-faint); font-weight: 600; text-transform: uppercase; letter-spacing: .04em; font-size: 11px; padding: 3px 10px 3px 0; vertical-align: top; }
  .cd-fields td { padding: 3px 0; font-family: ui-monospace, monospace; word-break: break-word; }
  .cd-body { border-top: 1px solid var(--border); padding-top: 10px; line-height: 1.55; font-size: 13.5px; }
  .cd-body :global(h1), .cd-body :global(h2), .cd-body :global(h3) { font-size: 15px; margin: 12px 0 6px; }
  .cd-body :global(code) { font-family: ui-monospace, monospace; background: rgba(128,128,128,0.15); padding: 1px 5px; border-radius: 4px; font-size: 12px; }
  .cd-body :global(pre) { background: rgba(128,128,128,0.12); padding: 8px 10px; border-radius: 6px; overflow: auto; }
  .cd-body :global(ul), .cd-body :global(ol) { padding-left: 20px; }
  .cd-body :global(a) { color: var(--accent); }
  .cd-msg { color: var(--text-dim); padding: 14px 0; font-size: 13px; }
  .cd-msg.err { color: #d05656; }
  .cd-msg.dim { color: var(--text-faint); }
  .cd-statusbar { flex: 0 0 auto; display: flex; align-items: center; gap: 14px; height: 26px; margin: 0 -18px 0; padding: 0 12px 0 18px; border-top: 1px solid var(--border); font-size: 11.5px; color: var(--text-dim); }
  .sb-loc { display: flex; align-items: center; gap: 4px; }
  .sb-meta { margin-left: auto; font-variant-numeric: tabular-nums; }
  .cd-grip { width: 14px; height: 14px; cursor: nwse-resize; flex: 0 0 auto; margin-right: -6px;
    background: repeating-linear-gradient(135deg, transparent 0 2px, var(--border-strong) 2px 3px); border-bottom-right-radius: 10px; }
</style>
