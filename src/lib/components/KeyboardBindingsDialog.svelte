<script lang="ts">
  /**
   * Keyboard shortcuts VIEWER + REBIND surface (CPE-1548 read-only base + CPE-1549's press-to-set
   * capture, live conflict warning, and reset-to-default — both epic CPE-1484 "hotkey
   * customization"). Mirrors `ShortcutsDialog.svelte`'s backdrop/dialog/Escape/click-away structure
   * and grouped-column layout (the same visual language as the "?" cheat sheet), but is driven by
   * `keymap.ts`'s live `ACTIONS` registry + a locally-owned, self-persisting `keymap` (seeded from
   * the caller-supplied prop) instead of the static `SHORTCUT_GROUPS` table. `ShortcutsDialog.svelte`
   * itself is untouched and stays as the quick "?" reference.
   *
   * Self-managing like every other Settings sub-dialog (`SpotlightHotkeySettings`,
   * `MacrosDialog`'s per-row hotkey field): every accepted rebind/reset calls `saveKeymap`
   * immediately — no separate Save/Cancel/Apply — matching the app's existing immediate-persist
   * Settings pattern. The `keymap` prop is a one-shot seed (`SettingsDialog` passes
   * `settings.loadKeymap()` un-bound), not a two-way binding; this component owns mutation +
   * persistence of its own local copy from there.
   *
   * Conflict handling: a rebind that would leave the new chord shared with another action is never
   * applied silently. `findConflicts` (CPE-1547) runs against the candidate keymap; a collision
   * shows an inline "Rebind anyway" / "Cancel" choice naming the other action. "Rebind anyway"
   * applies the new chord AND unbinds the other action (`setChord(..., "")`) — two actions never
   * silently share one chord. "Cancel" discards the candidate; both bindings stay exactly as they
   * were.
   *
   * NOT LIVE YET: `App.svelte`'s `handleKeydown` does not consult this keymap — that migration is
   * deliberately deferred to a future ticket (see the epic brief). A remap made here is saved and
   * survives restart, but doesn't change what the key actually does until that migration lands;
   * the note below the search bar says so.
   */
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import HotkeyCaptureInput from "./HotkeyCaptureInput.svelte";
  import {
    ACTIONS,
    chordFor,
    formatChord,
    setChord,
    resetChord,
    resetAll,
    findConflicts,
    exportKeymap,
    importKeymap,
    type ActionId,
    type Keymap,
  } from "../keymap";
  import { saveKeymap } from "../settings";

  export let keymap: Keymap;

  const dispatch = createEventDispatcher<{ close: void }>();

  let query = "";

  // Export / Import disclosure (CPE-1550) — collapsed by default, matching MacrosDialog's
  // showImport toggle pattern.
  let showIO = false;
  let importJson = "";
  let ioNote = "";
  let ioError = "";
  $: exportJson = exportKeymap(keymap);

  async function copyExport() {
    try {
      await navigator.clipboard.writeText(exportJson);
      ioError = "";
      ioNote = "Copied to clipboard.";
    } catch (e) {
      ioNote = "";
      ioError = e instanceof Error ? e.message : String(e);
    }
  }

  function runImport() {
    if (!importJson.trim()) return;
    const result = importKeymap(importJson, keymap);
    if (result.applied.length === 0 && result.rejected.length > 0) {
      ioNote = "";
      ioError = `Nothing applied — ${result.rejected.length} entr${result.rejected.length === 1 ? "y" : "ies"} rejected.`;
      return;
    }
    pendingConflict = null;
    applyKeymap(result.keymap);
    ioError = "";
    ioNote =
      result.rejected.length > 0
        ? `Applied ${result.applied.length}, skipped ${result.rejected.length} unrecognized.`
        : `Applied ${result.applied.length}.`;
    importJson = "";
  }

  // How many HotkeyCaptureInput rows are currently armed (almost always 0 or 1, but tracked as a
  // count rather than a bool in case a stray double-arm ever happens). While > 0, the dialog's own
  // Escape-to-close handler stands down so cancelling a capture doesn't also close the dialog —
  // see HotkeyCaptureInput's `armchange` doc comment. This listener is registered before any row's
  // (it appears earlier in the template), so it always observes the pre-cancel count for the same
  // Escape keydown.
  let capturingCount = 0;
  function handleArmChange(isArmed: boolean) {
    capturingCount = Math.max(0, capturingCount + (isArmed ? 1 : -1));
  }

  interface PendingConflict {
    id: ActionId;
    chord: string;
    collidingId: ActionId;
    collidingDescription: string;
  }
  let pendingConflict: PendingConflict | null = null;

  function applyKeymap(next: Keymap) {
    keymap = next;
    saveKeymap(keymap);
  }

  /** A capture input committed a new chord for `id`. Applies it immediately unless the resulting
   *  keymap now has a conflict touching `id`, in which case it's held as `pendingConflict` for the
   *  user to confirm or cancel — never applied silently. */
  function handleSet(id: ActionId, rawChord: string) {
    const candidate = setChord(keymap, id, rawChord);
    const chord = chordFor(candidate, id);
    const conflict = findConflicts(candidate).find((c) => c.ids.includes(id));
    if (conflict) {
      const collidingId = conflict.ids.find((x) => x !== id)!;
      const collidingDescription = ACTIONS.find((a) => a.id === collidingId)?.description ?? collidingId;
      pendingConflict = { id, chord, collidingId, collidingDescription };
      return;
    }
    pendingConflict = null;
    applyKeymap(candidate);
  }

  function confirmRebind() {
    if (!pendingConflict) return;
    const { id, chord, collidingId } = pendingConflict;
    let next = setChord(keymap, id, chord);
    next = setChord(next, collidingId, ""); // never let two actions silently share a chord
    pendingConflict = null;
    applyKeymap(next);
  }

  function cancelRebind() {
    pendingConflict = null;
  }

  function reset(id: ActionId) {
    if (pendingConflict && (pendingConflict.id === id || pendingConflict.collidingId === id)) {
      pendingConflict = null;
    }
    applyKeymap(resetChord(keymap, id));
  }

  function resetAllToDefaults() {
    pendingConflict = null;
    applyKeymap(resetAll());
  }

  // Group order follows ACTIONS' registry order (Navigation/Tabs/Selection/File actions/View/
  // General), same as ShortcutsDialog's SHORTCUT_GROUPS order — no separate sort needed.
  $: groups = (() => {
    const q = query.trim().toLowerCase();
    const order: string[] = [];
    const byGroup = new Map<string, { id: ActionId; description: string; chord: string }[]>();
    for (const action of ACTIONS) {
      const chord = chordFor(keymap, action.id);
      if (q && !action.description.toLowerCase().includes(q) && !action.group.toLowerCase().includes(q)) {
        continue;
      }
      if (!byGroup.has(action.group)) {
        byGroup.set(action.group, []);
        order.push(action.group);
      }
      byGroup.get(action.group)!.push({ id: action.id, description: action.description, chord });
    }
    return order.map((title) => ({ title, items: byGroup.get(title)! }));
  })();
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && capturingCount === 0 && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Keyboard shortcuts"
    on:click|stopPropagation
  >
    <h2>
      <span class="ic"><Icon name="keyboard" size={18} /></span>
      Keyboard shortcuts
      <button class="x" title="Close (Esc)" aria-label="Close" on:click={() => dispatch("close")}>
        <Icon name="close" size={16} />
      </button>
    </h2>

    <div class="search">
      <Icon name="search" size={14} />
      <input
        type="text"
        placeholder="Filter shortcuts…"
        bind:value={query}
        data-testid="keyboard-bindings-filter"
        spellcheck="false"
        autocomplete="off"
      />
      <button class="reset-all" data-testid="keyboard-bindings-reset-all" on:click={resetAllToDefaults}>
        <Icon name="refresh" size={12} />
        Reset all to defaults
      </button>
    </div>

    <div class="livenote">
      Rebinding here saves your choice, but shortcuts don't use it yet — that wiring is coming in a
      future update. For now every key still does what it always did.
    </div>

    <div class="groups" data-testid="keyboard-bindings-groups">
      {#if groups.length === 0}
        <div class="empty">No shortcuts match "{query}".</div>
      {:else}
        {#each groups as group (group.title)}
          <section>
            <h3>{group.title}</h3>
            {#each group.items as item (item.id)}
              <div class="row">
                <span class="desc">{item.description}</span>
                <HotkeyCaptureInput
                  display={item.chord ? formatChord(item.chord) : ""}
                  testId="hotkey-capture-{item.id}"
                  on:set={(e) => handleSet(item.id, e.detail)}
                  on:armchange={(e) => handleArmChange(e.detail)}
                />
                <button
                  class="reset-row"
                  title="Reset to default"
                  aria-label="Reset {item.description} to default"
                  data-testid="keyboard-binding-reset-{item.id}"
                  on:click={() => reset(item.id)}
                >
                  <Icon name="refresh" size={12} />
                </button>
              </div>
              {#if pendingConflict && pendingConflict.id === item.id}
                <div class="conflict" data-testid="keyboard-binding-conflict-{item.id}">
                  <Icon name="ban" size={13} />
                  <span
                    >This chord is already used by <strong>{pendingConflict.collidingDescription}</strong
                    >.</span
                  >
                  <button
                    class="btn danger"
                    data-testid="keyboard-binding-conflict-rebind-{item.id}"
                    on:click={confirmRebind}>Rebind anyway</button
                  >
                  <button
                    class="btn"
                    data-testid="keyboard-binding-conflict-cancel-{item.id}"
                    on:click={cancelRebind}>Cancel</button
                  >
                </div>
              {/if}
            {/each}
          </section>
        {/each}
      {/if}
    </div>

    <div class="io-section">
      <button
        class="io-toggle"
        data-testid="keymap-io-toggle"
        aria-expanded={showIO}
        on:click={() => (showIO = !showIO)}
      >
        <Icon name={showIO ? "chev-down" : "chev-right"} size={12} />
        Import / Export
      </button>
      {#if showIO}
        <div class="io-body">
          <div class="io-col">
            <label for="keymap-export-textarea">Export — current keymap as JSON</label>
            <textarea
              id="keymap-export-textarea"
              class="io-textarea"
              readonly
              data-testid="keymap-export-textarea"
              value={exportJson}
            ></textarea>
            <button class="btn" data-testid="keymap-export-copy-btn" on:click={copyExport}>
              Copy to clipboard
            </button>
          </div>
          <div class="io-col">
            <label for="keymap-import-textarea">Import — paste keymap JSON</label>
            <textarea
              id="keymap-import-textarea"
              class="io-textarea"
              placeholder="Paste exported keymap JSON…"
              bind:value={importJson}
              data-testid="keymap-import-textarea"
            ></textarea>
            <button
              class="btn primary"
              data-testid="keymap-import-btn"
              disabled={!importJson.trim()}
              on:click={runImport}
            >
              Import
            </button>
          </div>
        </div>
        <div class="io-status">
          {#if ioError}<span class="io-err" data-testid="keymap-io-error">{ioError}</span>
          {:else if ioNote}<span class="io-note" data-testid="keymap-io-note">{ioNote}</span>{/if}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    display: grid;
    place-items: center;
    z-index: 200;
  }
  .dialog {
    width: 860px;
    max-width: 92vw;
    max-height: 86vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 16px;
    margin-bottom: 14px;
  }
  .ic { display: grid; place-items: center; color: var(--accent); }
  .x { margin-left: auto; padding: 4px; border-radius: var(--radius); color: var(--text-dim); }
  .x:hover { background: var(--active); color: var(--text); }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    height: 32px;
    margin-bottom: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text-dim);
    flex: 0 0 auto;
  }
  .search input {
    flex: 1 1 auto;
    min-width: 0;
    height: 100%;
    border: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    outline: none;
  }
  .reset-all {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: 0 0 auto;
    height: 24px;
    padding: 0 8px;
    font-size: 11.5px;
    color: var(--text-dim);
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    white-space: nowrap;
  }
  .reset-all:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .livenote {
    font-size: 11.5px;
    color: var(--text-dim);
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 6px 10px;
    margin-bottom: 12px;
    flex: 0 0 auto;
  }
  .groups {
    overflow-y: auto;
    columns: 2;
    column-gap: 28px;
  }
  .empty {
    columns: initial;
    color: var(--text-dim);
    font-size: 12.5px;
    padding: 12px 2px;
  }
  section {
    break-inside: avoid;
    margin-bottom: 16px;
  }
  h3 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-dim);
    margin-bottom: 6px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 30px;
  }
  .desc {
    color: var(--text);
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-right: auto;
  }
  .reset-row {
    flex: 0 0 auto;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    color: var(--text-dim);
    border-radius: var(--radius);
  }
  .reset-row:hover {
    background: var(--active);
    color: var(--text);
  }
  .conflict {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    margin: 2px 0 8px;
    padding: 6px 8px;
    font-size: 12px;
    color: var(--text);
    background: var(--surface-alt);
    border: 1px solid var(--danger);
    border-radius: var(--radius);
  }
  .conflict :global(.icon) {
    color: var(--danger);
    flex: 0 0 auto;
  }
  .conflict span {
    flex: 1 1 auto;
    min-width: 140px;
  }
  .btn {
    flex: 0 0 auto;
    height: 22px;
    padding: 0 8px;
    font-size: 11.5px;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    white-space: nowrap;
  }
  .btn:hover {
    border-color: var(--accent);
  }
  .btn.danger {
    color: var(--danger);
    border-color: var(--danger);
  }
  .btn.danger:hover {
    background: var(--danger);
    color: var(--pal-white);
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--pal-white);
  }
  .io-section {
    flex: 0 0 auto;
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  .io-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-dim);
    padding: 2px 0;
  }
  .io-toggle:hover {
    color: var(--text);
  }
  .io-body {
    display: flex;
    gap: 16px;
    margin-top: 10px;
  }
  .io-col {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .io-col label {
    font-size: 11.5px;
    color: var(--text-dim);
  }
  .io-textarea {
    width: 100%;
    height: 100px;
    resize: vertical;
    font: 11px/1.4 var(--mono, monospace);
    color: var(--text);
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 6px 8px;
  }
  .io-textarea:read-only {
    color: var(--text-dim);
  }
  .io-status {
    min-height: 16px;
    margin-top: 6px;
    font-size: 11.5px;
  }
  .io-note {
    color: var(--text-dim);
  }
  .io-err {
    color: var(--danger);
  }
</style>
