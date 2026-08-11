<script lang="ts">
  // Binary Inspector (CPE-1597, epic CPE-1562 "Binary Inspector" slice 4): a tabbed, read-only preview
  // for executables/libraries (PE .exe/.dll/.sys/.efi/.ocx/.scr/.cpl, ELF .so, Mach-O .dylib), wiring the
  // CPE-1572/1581 `binaryInfo`/`binaryDisasm` backend commands into the preview pane. Self-contained like
  // CertPreview.svelte/JwtPreview.svelte/FontPreview.svelte: fetches its own data from `path`, no
  // prop-drilled callback, no declared action-bar `actions` (nothing here needs one).
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen";
  import type { BinaryInfo, BinaryInstruction } from "../bindings.gen";
  import { formatSize } from "../format";
  import Icon from "./Icon.svelte";
  import {
    capRows,
    classifyBinaryError,
    formatLabel,
    hexAddress,
    managedDotNetConfidence,
    BINARY_TABLE_ROW_CAP,
  } from "../preview/binaryInspector";

  /** The executable/library file's path. */
  export let path: string;
  /** The file's size in bytes — from the previewed `DirEntry` (`BinaryInfo` itself carries no file-size
   *  field, only structural data), shown on the Overview tab. */
  export let size = 0;

  type Tab = "overview" | "sections" | "imports" | "exports" | "symbols" | "disasm";
  let tab: Tab = "overview";

  let info: BinaryInfo | null = null;
  let loading = false;
  let loadError = "";

  // Reload whenever the previewed file changes (mirrors CertPreview/JwtPreview's `loadedPath` guard).
  let loadedPath = "";
  $: if (path && path !== loadedPath) {
    loadedPath = path;
    tab = "overview"; // never carry a stale tab selection across files
    resetDisasm();
    void load();
  }

  async function load() {
    loading = true;
    loadError = "";
    info = null;
    try {
      info = unwrap(await commands.binaryInfo(path));
    } catch (e) {
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  $: errorKind = loadError ? classifyBinaryError(loadError) : null;

  // ---- managed-.NET honesty (CPE-1597; TODO(CPE-1596): swap for the real backend flag once it lands) ---
  // Two-tier confidence rather than a plain flag, so the wording never asserts a guess as fact — see
  // `managedDotNetConfidence`'s own doc comment for why "possible" (zero imports AND zero exports) exists
  // at all: it's the ONLY signal that catches the real mscorlib.dll shape this ticket verified against.
  $: managedConfidence = info ? managedDotNetConfidence(info) : "none";
  $: managed = managedConfidence !== "none";
  // The user can still ask to see the raw (meaningless) x86/x64 decode of a managed assembly's CIL bytes
  // — transparency over silently withholding data — but it's opt-in and stays clearly labelled, never
  // shown as if it were real disassembly. Reset whenever the file changes (see resetDisasm below).
  let showManagedAnyway = false;

  // ---- lazy per-tab disassembly fetch (CPE-1597's #1 priority): `binaryDisasm` re-runs the ENTIRE
  // binary parse to get just the disasm list (~1.9x the cost of `binaryInfo` alone — measured 30.6ms vs
  // 16.3ms on a 24MB DLL), so it must only be called once the Disassembly tab is actually opened, never
  // eagerly alongside `binaryInfo` on selection. ----
  let disasm: BinaryInstruction[] | null = null;
  let disasmState: "idle" | "loading" | "error" = "idle";
  let disasmReqId = 0;

  function resetDisasm() {
    disasm = null;
    disasmState = "idle";
    disasmReqId += 1; // supersede any in-flight fetch for the file we're navigating away from
    showManagedAnyway = false;
  }

  // Fires only when the Disassembly tab becomes active, `info` has loaded, and nothing has been fetched
  // yet — switching tabs back and forth (or re-rendering for any other reason) never refetches.
  $: if (tab === "disasm" && info && !managed && disasm === null && disasmState === "idle") {
    void loadDisasm();
  }
  // The managed-anyway opt-in follows the same lazy rule: only fetch once the user actually asks to see it.
  $: if (tab === "disasm" && managed && showManagedAnyway && disasm === null && disasmState === "idle") {
    void loadDisasm();
  }

  async function loadDisasm() {
    const mine = ++disasmReqId;
    disasmState = "loading";
    try {
      const result = unwrap(await commands.binaryDisasm(path));
      if (mine !== disasmReqId) return; // stale — selection moved on while this was in flight
      disasm = result;
      disasmState = "idle";
    } catch {
      if (mine !== disasmReqId) return;
      disasmState = "error";
    }
  }

  // ---- table capping (CPE-1597's #3 priority): a system DLL can carry 1,000+ imports/exports; render
  // at most BINARY_TABLE_ROW_CAP rows per table and label the cap honestly rather than stalling the pane
  // on an unvirtualized table that large. ----
  $: sectionsCap = info ? capRows(info.sections) : null;
  $: importsCap = info ? capRows(info.imports) : null;
  $: exportsCap = info ? capRows(info.exports) : null;
  $: symbolsCap = info ? capRows(info.symbols) : null;

  function fmtCount(n: number): string {
    return n.toLocaleString();
  }
</script>

<div class="bp-preview" data-testid="binary-preview">
  {#if loading}
    <p class="bp-note">Loading…</p>
  {:else if loadError}
    {#if errorKind === "too-large"}
      <p class="bp-error" data-testid="binary-error-too-large">
        This file is larger than the preview limit and can't be inspected.
      </p>
    {:else if errorKind === "permission"}
      <p class="bp-error" data-testid="binary-error-permission">
        You don't have permission to read this file.
      </p>
    {:else}
      <p class="bp-error" data-testid="binary-error-unrecognized">
        This doesn't look like a recognized executable or library (PE, ELF, or Mach-O).
      </p>
    {/if}
    <p class="bp-error-detail">{loadError}</p>
  {:else if info}
    <div class="bp-banner">
      <Icon name="executable" size={14} />
      <span>Binary Inspector — a read-only view of the file's structure. It never executes, decompiles, or edits the file.</span>
    </div>

    <div class="bp-tabs" role="tablist">
      <button class="tab" class:active={tab === "overview"} role="tab" aria-selected={tab === "overview"} on:click={() => (tab = "overview")}>
        Overview
      </button>
      <button class="tab" class:active={tab === "sections"} role="tab" aria-selected={tab === "sections"} on:click={() => (tab = "sections")}>
        Sections ({fmtCount(info.sections.length)})
      </button>
      <button class="tab" class:active={tab === "imports"} role="tab" aria-selected={tab === "imports"} on:click={() => (tab = "imports")}>
        Imports ({fmtCount(info.imports.length)})
      </button>
      <button class="tab" class:active={tab === "exports"} role="tab" aria-selected={tab === "exports"} on:click={() => (tab = "exports")}>
        Exports ({fmtCount(info.exports.length)})
      </button>
      <button class="tab" class:active={tab === "symbols"} role="tab" aria-selected={tab === "symbols"} on:click={() => (tab = "symbols")}>
        Symbols ({fmtCount(info.symbols.length)})
      </button>
      <button class="tab" class:active={tab === "disasm"} role="tab" aria-selected={tab === "disasm"} on:click={() => (tab = "disasm")}>
        Disassembly
      </button>
    </div>

    <div class="bp-panel" data-testid="binary-tab-panel" data-tab={tab}>
      {#if tab === "overview"}
        <dl class="bp-rows">
          <div><dt>Format</dt><dd>{formatLabel(info.format)}</dd></div>
          <div><dt>Architecture</dt><dd>{info.arch ?? "Unknown"}</dd></div>
          <div><dt>Bitness</dt><dd>{info.is_64 ? "64-bit" : "32-bit"}</dd></div>
          <div><dt>File size</dt><dd>{formatSize(size)}</dd></div>
          <div><dt>Sections</dt><dd>{fmtCount(info.sections.length)}</dd></div>
          <div><dt>Imports</dt><dd>{fmtCount(info.imports.length)}</dd></div>
          <div><dt>Exports</dt><dd>{fmtCount(info.exports.length)}</dd></div>
          <div><dt>Symbols</dt><dd>{fmtCount(info.symbols.length)}</dd></div>
        </dl>
        {#if managedConfidence === "confirmed"}
          <div class="bp-banner warn" data-testid="binary-managed-badge">
            <Icon name="info" size={14} />
            <span>This looks like a managed .NET assembly — see the Disassembly tab for what that means for machine-code decoding.</span>
          </div>
        {:else if managedConfidence === "possible"}
          <div class="bp-banner warn" data-testid="binary-managed-badge">
            <Icon name="info" size={14} />
            <span>This file has no imports or exports — common for a managed .NET assembly with no classic native import table. See the Disassembly tab for what that would mean for machine-code decoding.</span>
          </div>
        {/if}
      {:else if tab === "sections"}
        {#if info.sections.length === 0}
          <p class="bp-empty" data-testid="binary-sections-empty">No sections found.</p>
        {:else if sectionsCap}
          <div class="bp-table-wrap">
            <table>
              <thead><tr><th>Name</th><th>Address</th><th class="num">Size</th></tr></thead>
              <tbody>
                {#each sectionsCap.rows as s}
                  <tr><td class="mono">{s.name}</td><td class="mono">{hexAddress(s.address)}</td><td class="num">{formatSize(s.size)}</td></tr>
                {/each}
              </tbody>
            </table>
          </div>
          {#if sectionsCap.capped}
            <p class="bp-cap-note" data-testid="binary-sections-capped">
              Showing the first {fmtCount(BINARY_TABLE_ROW_CAP)} of {fmtCount(sectionsCap.total)} sections — capped to keep the pane responsive.
            </p>
          {/if}
        {/if}
      {:else if tab === "imports"}
        {#if info.imports.length === 0}
          <p class="bp-empty" data-testid="binary-imports-empty">No imports found.</p>
        {:else if importsCap}
          <div class="bp-table-wrap">
            <table>
              <thead><tr><th>Name</th><th>Library</th></tr></thead>
              <tbody>
                {#each importsCap.rows as i}
                  <tr><td class="mono">{i.name}</td><td class="mono">{i.library ?? "—"}</td></tr>
                {/each}
              </tbody>
            </table>
          </div>
          {#if importsCap.capped}
            <p class="bp-cap-note" data-testid="binary-imports-capped">
              Showing the first {fmtCount(BINARY_TABLE_ROW_CAP)} of {fmtCount(importsCap.total)} imports — capped to keep the pane responsive.
            </p>
          {/if}
        {/if}
      {:else if tab === "exports"}
        {#if info.exports.length === 0}
          <p class="bp-empty" data-testid="binary-exports-empty">No exports found.</p>
        {:else if exportsCap}
          <div class="bp-table-wrap">
            <table>
              <thead><tr><th>Name</th><th>Address</th></tr></thead>
              <tbody>
                {#each exportsCap.rows as e}
                  <tr><td class="mono">{e.name}</td><td class="mono">{hexAddress(e.address)}</td></tr>
                {/each}
              </tbody>
            </table>
          </div>
          {#if exportsCap.capped}
            <p class="bp-cap-note" data-testid="binary-exports-capped">
              Showing the first {fmtCount(BINARY_TABLE_ROW_CAP)} of {fmtCount(exportsCap.total)} exports — capped to keep the pane responsive.
            </p>
          {/if}
        {/if}
      {:else if tab === "symbols"}
        {#if info.symbols.length === 0}
          <p class="bp-empty" data-testid="binary-symbols-empty">
            {info.format === "Pe"
              ? "No symbol table — a typical PE EXE/DLL doesn't carry one (only object files and PDBs do)."
              : "No symbols found."}
          </p>
        {:else if symbolsCap}
          <div class="bp-table-wrap">
            <table>
              <thead><tr><th>Name</th><th>Address</th></tr></thead>
              <tbody>
                {#each symbolsCap.rows as s}
                  <tr><td class="mono">{s.name}</td><td class="mono">{hexAddress(s.address)}</td></tr>
                {/each}
              </tbody>
            </table>
          </div>
          {#if symbolsCap.capped}
            <p class="bp-cap-note" data-testid="binary-symbols-capped">
              Showing the first {fmtCount(BINARY_TABLE_ROW_CAP)} of {fmtCount(symbolsCap.total)} symbols — capped to keep the pane responsive.
            </p>
          {/if}
        {/if}
      {:else if tab === "disasm"}
        {#if managed && !showManagedAnyway}
          <div class="bp-banner warn" data-testid="binary-managed-disasm-caveat">
            <Icon name="info" size={14} />
            {#if managedConfidence === "confirmed"}
              <span>
                This looks like a <strong>managed .NET assembly</strong>. Its code section holds Common
                Intermediate Language (CIL) bytecode, not native machine code — decoding it as x86/x64
                would produce meaningless output (confirmed on a real assembly: 2,048 nonsense
                "instructions"), so it isn't shown here.
              </span>
            {:else}
              <span>
                This file has <strong>no imports or exports</strong> — common for a managed .NET assembly
                loaded straight off its CLR header rather than a native import table (it isn't definitive;
                an unusual native binary can also have empty tables). If it IS managed, its code section is
                Common Intermediate Language (CIL) bytecode, not native machine code, and decoding it as
                x86/x64 would produce meaningless output — so it isn't shown here automatically.
              </span>
            {/if}
          </div>
          <button class="bp-btn" data-testid="binary-managed-show-anyway" on:click={() => (showManagedAnyway = true)}>
            Show the raw x86/x64 decode anyway (not meaningful)
          </button>
        {:else}
          {#if managed}
            <div class="bp-banner warn" data-testid="binary-managed-disasm-reminder">
              <Icon name="info" size={14} />
              <span>
                {managedConfidence === "confirmed"
                  ? "Reminder: this is CIL bytecode decoded as x86/x64 — it is not real disassembly."
                  : "Reminder: this file has no imports/exports, so this MAY be CIL bytecode decoded as x86/x64 rather than real disassembly."}
              </span>
            </div>
          {/if}
          {#if disasmState === "loading"}
            <p class="bp-note">Loading…</p>
          {:else if disasmState === "error"}
            <p class="bp-error" data-testid="binary-disasm-error">Couldn't decode this file's disassembly.</p>
          {:else if disasm && disasm.length === 0}
            <p class="bp-empty" data-testid="binary-disasm-empty">
              No disassembly available — an unsupported architecture (only x86/x64 is decoded), or no
              locatable code section.
            </p>
          {:else if disasm}
            <div class="bp-table-wrap">
              <table>
                <thead><tr><th>Address</th><th>Bytes</th><th>Instruction</th></tr></thead>
                <tbody>
                  {#each disasm as ins}
                    <tr><td class="mono">{hexAddress(ins.address)}</td><td class="mono">{ins.bytes}</td><td class="mono">{ins.text}</td></tr>
                  {/each}
                </tbody>
              </table>
            </div>
            <p class="bp-cap-note" data-testid="binary-disasm-note">
              Showing {fmtCount(disasm.length)} decoded instructions — x86/x64 only, capped at 2,048 by the backend.
            </p>
          {/if}
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .bp-preview { padding: 12px; font-size: 12px; }
  .bp-note { color: var(--text-faint); }
  .bp-error { color: var(--danger); }
  .bp-error-detail {
    margin-top: 4px; color: var(--text-faint); font-size: 11px; font-family: var(--mono, ui-monospace, monospace);
    overflow-wrap: anywhere;
  }
  .bp-empty { color: var(--text-faint); padding: 8px 0; }
  .bp-banner {
    display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-radius: var(--radius);
    background: var(--surface-alt); border: 1px solid var(--border); color: var(--text-dim);
    margin-bottom: 12px; font-size: 11.5px;
  }
  .bp-banner.warn {
    color: var(--text); border-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 8%, var(--surface));
  }
  /* Reuses the app-wide tab convention (docs/design/TABS.md): global `.tab`/`.tab.active` from
     src/app.css supply the accent-top-bar active tab + recessed-chip inactive treatment; this local
     wrapper just adapts the strip to the narrow preview pane (horizontal scroll instead of the main
     tabbar's fixed-width wrapping, tighter sizing). */
  .bp-tabs {
    display: flex; gap: 2px; overflow-x: auto; padding-bottom: 0; margin-bottom: 10px;
    border-bottom: 1px solid var(--border); flex: none;
  }
  .bp-tabs :global(.tab) {
    min-width: 0; max-width: none; flex: none; white-space: nowrap;
    height: 28px; padding: 0 10px; font-size: 11.5px;
  }
  .bp-rows { display: grid; gap: 6px; }
  .bp-rows > div { display: flex; gap: 10px; align-items: baseline; }
  .bp-rows dt { color: var(--text-dim); width: 110px; flex: none; }
  .bp-rows dd { flex: 1; overflow-wrap: anywhere; }
  .mono { font-family: var(--mono, ui-monospace, monospace); font-size: 11.5px; }
  .bp-table-wrap { overflow: auto; max-height: 420px; border: 1px solid var(--border); border-radius: var(--radius); }
  .bp-table-wrap table { width: 100%; border-collapse: collapse; }
  .bp-table-wrap th {
    position: sticky; top: 0; text-align: left; background: var(--surface-alt); color: var(--text-dim);
    font-weight: 600; padding: 5px 8px; border-bottom: 1px solid var(--border); font-size: 11px;
  }
  .bp-table-wrap td { padding: 4px 8px; border-bottom: 1px solid var(--border); overflow-wrap: anywhere; }
  .bp-table-wrap tr:last-child td { border-bottom: none; }
  .bp-table-wrap th.num, .bp-table-wrap td.num { text-align: right; }
  .bp-cap-note { margin: 8px 0 0; color: var(--text-faint); font-size: 11px; }
  .bp-btn {
    display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 10px;
    border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt);
    color: var(--text); font-size: 11.5px; cursor: pointer;
  }
  .bp-btn:hover { background: var(--surface); }
</style>
