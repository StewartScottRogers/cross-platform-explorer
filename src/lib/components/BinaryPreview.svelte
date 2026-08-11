<script lang="ts">
  // Binary Inspector (CPE-1597, epic CPE-1562 "Binary Inspector" slice 4; CPE-1615 slice 5): a tabbed,
  // read-only preview for executables/libraries (PE .exe/.dll/.sys/.efi/.ocx/.scr/.cpl, ELF .so, Mach-O
  // .dylib), wiring the CPE-1572/1581/1596 `binaryInfo`/`binaryDisasm`/`dotnetMetadata` backend commands
  // into the preview pane. Self-contained like CertPreview.svelte/JwtPreview.svelte/FontPreview.svelte:
  // fetches its own data from `path`, no prop-drilled callback, no declared action-bar `actions` (nothing
  // here needs one).
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen";
  import type { BinaryInfo, BinaryInstruction, DotnetMetadata } from "../bindings.gen";
  import { formatSize } from "../format";
  import Icon from "./Icon.svelte";
  import {
    capRows,
    classifyBinaryError,
    formatLabel,
    hexAddress,
    decodeAssemblyFlags,
    rawAssemblyFlags,
    cultureLabel,
    hexOrDash,
    BINARY_TABLE_ROW_CAP,
  } from "../preview/binaryInspector";

  /** The executable/library file's path. */
  export let path: string;
  /** The file's size in bytes — from the previewed `DirEntry` (`BinaryInfo` itself carries no file-size
   *  field, only structural data), shown on the Overview tab. */
  export let size = 0;

  type Tab = "overview" | "sections" | "imports" | "exports" | "symbols" | "dotnet" | "disasm";
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
    resetDotnet();
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

  // ---- managed-.NET gating (CPE-1615) ---------------------------------------------------------------
  // `info.is_managed` is a real CLR-header read (the PE optional header's IMAGE_COR20_HEADER data
  // directory), computed backend-side by `binary_info` — not a guess — so it gates both the ".NET
  // metadata" tab and the Disassembly tab's CIL-vs-x86 caveat directly, with no hedged wording needed.
  // (This used to be a frontend-side heuristic, `managedDotNetConfidence`, that guessed from
  // imports/exports/extension; retired now that the real flag exists — see CPE-1615.)
  $: managed = !!info && info.is_managed;
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

  // ---- lazy .NET metadata fetch (CPE-1615) ----------------------------------------------------------
  // Mirrors the Disassembly tab's lazy-fetch-by-request-id pattern above: `dotnetMetadata` re-walks the
  // CLR `#~` table stream, a heavier parse than `binaryInfo` alone, so it's only fetched once the ".NET
  // metadata" tab is actually opened. `dotnetState` carries an explicit "loaded" tier (unlike
  // `disasmState`, which reuses "idle" for its always-non-null empty-array result) because the command's
  // success value IS nullable — `null` means "a real, valid response saying there's no parseable metadata
  // root" (e.g. a module rather than an assembly manifest), which must render as a distinct, explained
  // state, never mistaken for "not fetched yet" (which would refetch forever) or lumped in with a genuine
  // fetch failure (a thrown error — malformed/unreadable file).
  let dotnetMeta: DotnetMetadata | null = null;
  let dotnetState: "idle" | "loading" | "loaded" | "error" = "idle";
  let dotnetReqId = 0;

  function resetDotnet() {
    dotnetMeta = null;
    dotnetState = "idle";
    dotnetReqId += 1; // supersede any in-flight fetch for the file we're navigating away from
  }

  $: if (tab === "dotnet" && managed && dotnetState === "idle") {
    void loadDotnet();
  }

  async function loadDotnet() {
    const mine = ++dotnetReqId;
    dotnetState = "loading";
    try {
      const result = unwrap(await commands.dotnetMetadata(path));
      if (mine !== dotnetReqId) return; // stale — selection moved on while this was in flight
      dotnetMeta = result;
      dotnetState = "loaded";
    } catch {
      if (mine !== dotnetReqId) return;
      dotnetState = "error";
    }
  }

  // ---- table capping (CPE-1597's #3 priority; extended to the .NET tables by CPE-1615): a system DLL
  // can carry 1,000+ imports/exports, and a large managed assembly can likewise carry thousands of
  // AssemblyRefs/types/methods — render at most BINARY_TABLE_ROW_CAP rows per table and label the cap
  // honestly rather than stalling the pane on an unvirtualized table that large. ----
  $: sectionsCap = info ? capRows(info.sections) : null;
  $: importsCap = info ? capRows(info.imports) : null;
  $: exportsCap = info ? capRows(info.exports) : null;
  $: symbolsCap = info ? capRows(info.symbols) : null;
  $: assemblyRefsCap = dotnetMeta ? capRows(dotnetMeta.assembly_refs) : null;
  $: typesCap = dotnetMeta ? capRows(dotnetMeta.types) : null;
  $: methodsCap = dotnetMeta ? capRows(dotnetMeta.methods) : null;
  $: assemblyFlags = dotnetMeta?.assembly ? decodeAssemblyFlags(dotnetMeta.assembly.flags) : [];

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
      {#if managed}
        <button class="tab" class:active={tab === "dotnet"} role="tab" aria-selected={tab === "dotnet"} on:click={() => (tab = "dotnet")}>
          .NET metadata
        </button>
      {/if}
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
        {#if managed}
          <div class="bp-banner warn" data-testid="binary-managed-badge">
            <Icon name="info" size={14} />
            <span>This is a managed .NET assembly — see the .NET metadata tab for its assembly identity, referenced assemblies, and types, and the Disassembly tab for what that means for machine-code decoding.</span>
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
      {:else if tab === "dotnet"}
        {#if dotnetState === "loading"}
          <p class="bp-note">Loading…</p>
        {:else if dotnetState === "error"}
          <p class="bp-error" data-testid="binary-dotnet-error">Couldn't read this assembly's .NET metadata.</p>
        {:else if dotnetState === "loaded" && dotnetMeta === null}
          <!-- Structurally distinct from an empty-but-valid result (below): a real, successful response
               that found no parseable metadata root at all — never rendered as a clean/empty table. -->
          <p class="bp-empty" data-testid="binary-dotnet-null">
            No .NET metadata found — this file's CLR header is present, but its metadata root couldn't be
            located or parsed.
          </p>
        {:else if dotnetMeta}
          <section class="bp-dotnet-section">
            <h4>Assembly identity</h4>
            {#if dotnetMeta.assembly}
              <dl class="bp-rows">
                <div><dt>Name</dt><dd class="mono">{dotnetMeta.assembly.name}</dd></div>
                <div><dt>Version</dt><dd class="mono">{dotnetMeta.assembly.version}</dd></div>
                <div><dt>Culture</dt><dd>{cultureLabel(dotnetMeta.assembly.culture)}</dd></div>
                <div><dt>Public key</dt><dd class="mono">{hexOrDash(dotnetMeta.assembly.public_key)}</dd></div>
                <div>
                  <dt>Flags</dt>
                  <dd class="mono" data-testid="binary-dotnet-flags-raw">
                    {rawAssemblyFlags(dotnetMeta.assembly.flags)}
                  </dd>
                </div>
              </dl>
              {#if assemblyFlags.length > 0}
                <div class="bp-pills" data-testid="binary-dotnet-flags">
                  {#each assemblyFlags as f}
                    <span class="bp-pill">{f}</span>
                  {/each}
                </div>
              {/if}
            {:else}
              <p class="bp-empty" data-testid="binary-dotnet-no-assembly">
                No assembly manifest — this is a module, not a standalone assembly.
              </p>
            {/if}
            <p class="bp-cap-note">Compiled against runtime {dotnetMeta.runtime_version}.</p>
          </section>

          <section class="bp-dotnet-section">
            <h4>Referenced assemblies ({fmtCount(dotnetMeta.assembly_refs.length)})</h4>
            {#if dotnetMeta.assembly_refs.length === 0}
              <p class="bp-empty" data-testid="binary-dotnet-refs-empty">No referenced assemblies found.</p>
            {:else if assemblyRefsCap}
              <div class="bp-table-wrap">
                <table>
                  <thead><tr><th>Name</th><th>Version</th><th>Culture</th><th>Public key token</th></tr></thead>
                  <tbody>
                    {#each assemblyRefsCap.rows as r}
                      <tr><td class="mono">{r.name}</td><td class="mono">{r.version}</td><td>{cultureLabel(r.culture)}</td><td class="mono">{hexOrDash(r.public_key_token)}</td></tr>
                    {/each}
                  </tbody>
                </table>
              </div>
              {#if assemblyRefsCap.capped}
                <p class="bp-cap-note" data-testid="binary-dotnet-refs-capped">
                  Showing the first {fmtCount(BINARY_TABLE_ROW_CAP)} of {fmtCount(assemblyRefsCap.total)} referenced assemblies — capped to keep the pane responsive.
                </p>
              {/if}
            {/if}
          </section>

          <section class="bp-dotnet-section">
            <h4>Types ({fmtCount(dotnetMeta.types.length)})</h4>
            {#if dotnetMeta.types.length === 0}
              <p class="bp-empty" data-testid="binary-dotnet-types-empty">No types found.</p>
            {:else if typesCap}
              <div class="bp-table-wrap">
                <table>
                  <thead><tr><th>Namespace</th><th>Name</th></tr></thead>
                  <tbody>
                    {#each typesCap.rows as t}
                      <tr><td class="mono">{t.namespace || "—"}</td><td class="mono">{t.name}</td></tr>
                    {/each}
                  </tbody>
                </table>
              </div>
              {#if typesCap.capped}
                <p class="bp-cap-note" data-testid="binary-dotnet-types-capped">
                  Showing the first {fmtCount(BINARY_TABLE_ROW_CAP)} of {fmtCount(typesCap.total)} types — capped to keep the pane responsive.
                </p>
              {/if}
            {/if}
          </section>

          <section class="bp-dotnet-section">
            <h4>Methods ({fmtCount(dotnetMeta.methods.length)})</h4>
            {#if dotnetMeta.methods.length === 0}
              <p class="bp-empty" data-testid="binary-dotnet-methods-empty">No methods found.</p>
            {:else if methodsCap}
              <div class="bp-table-wrap">
                <table>
                  <thead><tr><th>Name</th></tr></thead>
                  <tbody>
                    {#each methodsCap.rows as m}
                      <tr><td class="mono">{m.name}</td></tr>
                    {/each}
                  </tbody>
                </table>
              </div>
              {#if methodsCap.capped}
                <p class="bp-cap-note" data-testid="binary-dotnet-methods-capped">
                  Showing the first {fmtCount(BINARY_TABLE_ROW_CAP)} of {fmtCount(methodsCap.total)} methods — capped to keep the pane responsive.
                </p>
              {/if}
            {/if}
          </section>
        {/if}
      {:else if tab === "disasm"}
        {#if managed && !showManagedAnyway}
          <div class="bp-banner warn" data-testid="binary-managed-disasm-caveat">
            <Icon name="info" size={14} />
            <span>
              This is a <strong>managed .NET assembly</strong>. Its code section holds Common
              Intermediate Language (CIL) bytecode, not native machine code — decoding it as x86/x64
              would produce meaningless output (confirmed on a real assembly: 2,048 nonsense
              "instructions"), so it isn't shown here.
            </span>
          </div>
          <button class="bp-btn" data-testid="binary-managed-show-anyway" on:click={() => (showManagedAnyway = true)}>
            Show the raw x86/x64 decode anyway (not meaningful)
          </button>
        {:else}
          {#if managed}
            <div class="bp-banner warn" data-testid="binary-managed-disasm-reminder">
              <Icon name="info" size={14} />
              <span>Reminder: this is CIL bytecode decoded as x86/x64 — it is not real disassembly.</span>
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
  /* ".NET metadata" tab (CPE-1615): one section per table, each with its own heading. */
  .bp-dotnet-section { margin-bottom: 18px; }
  .bp-dotnet-section:last-child { margin-bottom: 0; }
  .bp-dotnet-section h4 { margin: 0 0 8px; font-size: 12px; font-weight: 600; color: var(--text-dim); }
  /* Assembly-flags pill row (CPE-1615) — must reflow, never let a pill's text wrap and overflow its
     background (project-wide "tick-tacks" convention). */
  .bp-pills { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
  .bp-pill {
    display: inline-flex; align-items: center; white-space: nowrap; flex: 0 0 auto;
    padding: 2px 9px; border-radius: 999px; background: var(--surface-alt); border: 1px solid var(--border);
    color: var(--text); font-size: 11px;
  }
</style>
