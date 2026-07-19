<script lang="ts">
  // Application → Documents viewer (CPE-537): a TOC sidebar over the built-in docs library (CPE-536),
  // the selected doc rendered as sanitized markdown (reuse the preview renderer), and a search box.
  // Offline — the docs are bundled into the app at build time.
  import { createEventDispatcher, onMount, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { DOCS, searchDocs, groupDocs, type Doc } from "../docs";
  import { renderMarkdown } from "../preview/markdown";

  const dispatch = createEventDispatcher<{ close: void }>();

  // Optional deep-link: open on this doc slug (CPE-594). Unknown/absent → the default first doc, so
  // existing callers that pass nothing are unaffected. The viewer stays dumb — the caller/registry
  // decides which slug to pass.
  export let initialSlug: string | null = null;

  let query = "";
  let selected: Doc | null = (initialSlug ? DOCS.find((d) => d.slug === initialSlug) : null) ?? DOCS[0] ?? null;
  let html = "";
  // Per-category collapse state (name → collapsed?). Unset = open, so the whole library shows by default;
  // the left pane groups docs into expandable sections so it scales to many more pages (CPE-763).
  let collapsed: Record<string, boolean> = {};
  // Element per TOC item so a deep-link can scroll its section into view.
  let itemEls: Record<string, HTMLElement> = {};

  $: results = searchDocs(DOCS, query);
  $: groups = groupDocs(results);
  $: searching = query.trim().length > 0;
  // Keep the selection valid as the filter narrows.
  $: if (selected && !results.some((d) => d.slug === selected!.slug) && results.length) selected = results[0];
  $: render(selected);

  // While searching, force every group open so a match is never hidden behind a collapsed header.
  const isExpanded = (name: string): boolean => searching || !collapsed[name];
  const toggle = (name: string) => (collapsed = { ...collapsed, [name]: !collapsed[name] });

  async function render(doc: Doc | null) {
    html = doc ? await renderMarkdown(doc.content) : "";
  }

  // Deep-link (CPE-596/763): opened on a specific section, scroll its TOC item into view so "open into
  // any section from anywhere" actually lands you there (its category is expanded by default).
  onMount(async () => {
    await tick();
    if (selected) itemEls[selected.slug]?.scrollIntoView({ block: "nearest" });
  });
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="docs-overlay" on:click|self={() => dispatch("close")}>
  <div class="docs-panel">
    <div class="docs-titlebar">
      <span class="docs-title"><Icon name="document" size={15} /> Documents</span>
      <button class="docs-x" title="Close" aria-label="Close" on:click={() => dispatch("close")}>×</button>
    </div>

    {#if DOCS.length === 0}
      <div class="docs-empty">No documents are bundled in this build.</div>
    {:else}
      <div class="docs-body">
        <aside class="docs-toc">
          <input class="docs-search" placeholder="Search the docs…" bind:value={query} spellcheck="false" />
          {#each groups as g (g.name)}
            <div class="toc-group">
              <button
                class="toc-cat"
                aria-expanded={isExpanded(g.name)}
                on:click={() => toggle(g.name)}
                title={isExpanded(g.name) ? "Collapse section" : "Expand section"}
              >
                <Icon name={isExpanded(g.name) ? "chev-down" : "chev-right"} size={12} />
                <span class="toc-cat-name">{g.name}</span>
                <span class="toc-cat-count">{g.docs.length}</span>
              </button>
              {#if isExpanded(g.name)}
                {#each g.docs as d (d.slug)}
                  <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events -->
                  <div
                    class="toc-item"
                    class:sel={selected?.slug === d.slug}
                    bind:this={itemEls[d.slug]}
                    on:click={() => (selected = d)}
                  >
                    {d.title}
                  </div>
                {/each}
              {/if}
            </div>
          {/each}
          {#if results.length === 0}<div class="toc-empty">No matches.</div>{/if}
        </aside>
        <section class="docs-content">
          {#if selected}
            <!-- Markdown is sanitized by renderMarkdown (marked + DOMPurify). -->
            <div class="md">{@html html}</div>
          {:else}
            <div class="docs-empty">Pick a document on the left.</div>
          {/if}
        </section>
      </div>
    {/if}
  </div>
</div>

<style>
  .docs-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.45); display: flex;
    align-items: center; justify-content: center; z-index: 60; }
  .docs-panel { width: min(980px, 95vw); height: min(720px, 92vh); display: flex; flex-direction: column;
    background: var(--surface); color: var(--text); border: 1px solid var(--border-strong);
    border-radius: 8px; box-shadow: 0 16px 48px rgba(0,0,0,0.4); overflow: hidden; }

  .docs-titlebar { display: flex; align-items: center; justify-content: space-between;
    padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .docs-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .docs-x { border: 0; background: transparent; color: var(--text-dim); font-size: 20px; cursor: pointer;
    line-height: 1; padding: 0 4px; border-radius: 4px; }
  .docs-x:hover { background: rgba(128,128,128,0.18); color: var(--text); }
  .docs-empty { flex: 1; display: grid; place-items: center; color: var(--text-dim); }

  .docs-body { flex: 1; display: flex; min-height: 0; }
  .docs-toc { width: 240px; flex: 0 0 auto; border-right: 1px solid var(--border); overflow-y: auto; padding: 8px; }
  .docs-search { width: 100%; height: 30px; padding: 0 9px; margin-bottom: 8px; box-sizing: border-box;
    border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text); font: inherit; }
  .docs-search:focus { outline: none; border-color: var(--accent); }
  .toc-group { margin-bottom: 2px; }
  .toc-cat { display: flex; align-items: center; gap: 6px; width: 100%; padding: 6px 8px; border: 0;
    background: transparent; color: var(--text-dim); cursor: pointer; border-radius: 6px; font: inherit;
    font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em; }
  .toc-cat:hover { background: rgba(128,128,128,0.10); color: var(--text); }
  .toc-cat-name { flex: 1; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .toc-cat-count { flex: 0 0 auto; font-size: 10px; font-weight: 600; opacity: 0.6;
    font-variant-numeric: tabular-nums; }
  .toc-item { padding: 7px 9px 7px 24px; border-radius: 6px; cursor: pointer; font-size: 13px; }
  .toc-item:hover { background: rgba(128,128,128,0.12); }
  .toc-item.sel { background: var(--selection, rgba(128,128,128,0.22)); font-weight: 600; }
  .toc-empty { padding: 10px 9px; color: var(--text-faint); font-size: 12px; }

  .docs-content { flex: 1; overflow-y: auto; padding: 18px 26px; }
  /* Markdown typography (scoped to the rendered doc). */
  .md :global(h1) { font-size: 22px; margin: 0 0 12px; }
  .md :global(h2) { font-size: 17px; margin: 22px 0 8px; border-bottom: 1px solid var(--border); padding-bottom: 4px; }
  .md :global(h3) { font-size: 14px; margin: 16px 0 6px; }
  .md :global(p), .md :global(li) { line-height: 1.6; }
  .md :global(ul), .md :global(ol) { padding-left: 22px; }
  .md :global(code) { background: var(--surface-alt); padding: 1px 5px; border-radius: 4px; font-size: 12px; }
  .md :global(a) { color: var(--accent); }
  .md :global(strong) { color: var(--text); }
</style>
