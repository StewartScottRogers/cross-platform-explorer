<script lang="ts">
  /**
   * Hex/binary inspector preview (CPE-773, epic CPE-719). A paged offset/hex/ASCII grid over
   * `read_file_range` (CPE-772) — never loads the whole file — with a magic-byte signature badge
   * (`detectSignature`, CPE-770) and a data-inspector panel decoding the byte under the cursor
   * (`inspect`, CPE-771). Read-only v1; thin render over the tested pure helpers.
   */
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import { hexRows, detectSignature, type HexRow, type Signature } from "../hexdump";
  import { inspect, type InspectRow } from "../hexinspect";

  export let path: string;
  export let size = 0;

  /** Bytes per page (64 rows × 16). Paging keeps a large file from loading whole. */
  const PAGE = 1024;

  let pageOffset = 0;
  let bytes = new Uint8Array(0);
  let rows: HexRow[] = [];
  let sig: Signature | null = null;
  let cursor = 0;
  let littleEndian = true;
  let state: "loading" | "error" | "ok" = "loading";
  let error = "";
  let loadedPath = "";

  // Reset + load page 0 whenever the previewed file changes.
  $: if (path && path !== loadedPath) {
    loadedPath = path;
    pageOffset = 0;
    cursor = 0;
    sig = null;
    load(0);
  }

  async function load(offset: number) {
    state = "loading";
    error = "";
    try {
      const arr = unwrap(await commands.readFileRange(path, offset, PAGE));
      bytes = new Uint8Array(arr);
      rows = hexRows(bytes, offset);
      if (offset === 0) sig = detectSignature(bytes);
      pageOffset = offset;
      if (cursor < offset || cursor >= offset + bytes.length) cursor = offset;
      state = "ok";
    } catch (e) {
      error = String(e);
      state = "error";
    }
  }

  const hex2 = (b: number) => b.toString(16).padStart(2, "0").toUpperCase();
  function rowBytes(r: number): number[] {
    return Array.from(bytes.slice(r * 16, r * 16 + 16));
  }

  $: canPrev = pageOffset > 0;
  $: canNext = pageOffset + PAGE < size;
  $: inspectRows = (cursor >= pageOffset && cursor < pageOffset + bytes.length
    ? inspect(bytes, cursor - pageOffset, littleEndian)
    : []) as InspectRow[];
</script>

<div class="hexview" data-testid="hexview">
  <div class="bar">
    {#if sig}<span class="sig" data-testid="hex-sig">{sig.name} (.{sig.ext})</span>{:else}<span class="sig unknown">unknown format</span>{/if}
    <span class="spacer" />
    <button class="pg" data-testid="hex-prev" disabled={!canPrev || state === "loading"} on:click={() => load(Math.max(0, pageOffset - PAGE))}>◀ prev</button>
    <span class="range" data-testid="hex-range">0x{pageOffset.toString(16).toUpperCase()}–0x{(pageOffset + bytes.length).toString(16).toUpperCase()} / {size}b</span>
    <button class="pg" data-testid="hex-next" disabled={!canNext || state === "loading"} on:click={() => load(pageOffset + PAGE)}>next ▶</button>
  </div>

  {#if state === "error"}
    <p class="note">{error}</p>
  {:else}
    <div class="body">
      <div class="grid" data-testid="hex-grid">
        {#each rows as row, r (row.offset)}
          <div class="hrow">
            <span class="off">{row.offset}</span>
            <span class="cells">
              {#each rowBytes(r) as b, i (i)}
                <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
                <span class="cell" class:sel={cursor === pageOffset + r * 16 + i} data-off={pageOffset + r * 16 + i} on:click={() => (cursor = pageOffset + r * 16 + i)}>{hex2(b)}</span>
              {/each}
            </span>
            <span class="ascii">{row.ascii}</span>
          </div>
        {/each}
      </div>

      <div class="inspector" data-testid="hex-inspector">
        <div class="ins-head">
          <span>Byte 0x{cursor.toString(16).toUpperCase()}</span>
          <label class="le"><input type="checkbox" bind:checked={littleEndian} /> LE</label>
        </div>
        {#each inspectRows as row (row.type)}
          <div class="ins-row"><span class="ins-type">{row.type}</span><span class="ins-val">{row.value}</span></div>
        {/each}
        {#if inspectRows.length === 0}<div class="note">—</div>{/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .hexview { display: flex; flex-direction: column; height: 100%; font-size: 12px; }
  .bar { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .sig { padding: 1px 8px; border-radius: 999px; background: var(--accent); color: #fff; font-size: 11px; white-space: nowrap; }
  .sig.unknown { background: var(--surface-alt); color: var(--text-dim); }
  .spacer { flex: 1 1 auto; }
  .pg { height: 24px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); }
  .pg:disabled { opacity: 0.4; }
  .range { font-family: ui-monospace, monospace; color: var(--text-dim); font-size: 11px; white-space: nowrap; }
  .body { flex: 1 1 auto; display: flex; min-height: 0; }
  .grid { flex: 1 1 auto; overflow: auto; padding: 6px 8px; font-family: ui-monospace, monospace; }
  .hrow { display: flex; gap: 12px; white-space: pre; line-height: 1.5; }
  .off { color: var(--text-dim); }
  .cells { display: inline-flex; gap: 3px; }
  .cell { cursor: pointer; padding: 0 1px; border-radius: 2px; }
  .cell:hover { background: var(--surface-alt); }
  .cell.sel { background: var(--accent); color: #fff; }
  .ascii { color: var(--text); }
  .inspector { flex: 0 0 190px; border-left: 1px solid var(--border); overflow: auto; padding: 8px; }
  .ins-head { display: flex; align-items: center; justify-content: space-between; font-family: ui-monospace, monospace; color: var(--text-dim); margin-bottom: 6px; }
  .le { font-size: 11px; }
  .ins-row { display: flex; justify-content: space-between; gap: 8px; padding: 1px 0; }
  .ins-type { color: var(--text-dim); }
  .ins-val { font-family: ui-monospace, monospace; text-align: right; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .note { padding: 10px; color: var(--text-dim); }
</style>
