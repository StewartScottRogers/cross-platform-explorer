<script lang="ts">
  /**
   * Capability consent sheet (CPE-296). Shown the first time a sidecar requests
   * capabilities (or when an update adds a new one): the user sees each requested
   * capability with a plain-language description and a risk note, and grants or denies
   * per capability. The decision is persisted; the broker enforces it. Denial degrades
   * the sidecar gracefully — it never crashes it.
   */
  import { createEventDispatcher } from "svelte";
  import { CAPABILITY_INFO, type Capability, type ConsentState } from "../sidecar";

  export let sidecarId: string;
  export let state: ConsentState;

  const dispatch = createEventDispatcher<{
    decide: { granted: Capability[]; decided: Capability[] };
    cancel: void;
  }>();

  // Everything the sidecar asks for is shown. Pre-select capabilities already granted;
  // for undecided ones, default-select the non-sensitive and leave sensitive (secrets,
  // network) unchecked so granting them is a deliberate act.
  const granted0 = new Set(state.granted);
  let checked: Record<string, boolean> = {};
  for (const cap of state.requested) {
    // Already granted → on; otherwise default-on for non-sensitive, off for sensitive.
    checked[cap] = granted0.has(cap) ? true : !CAPABILITY_INFO[cap].sensitive;
  }

  function decide() {
    const decided = state.requested;
    const grantedSel = decided.filter((c) => checked[c]);
    dispatch("decide", { granted: grantedSel, decided });
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />
<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="consent-backdrop" on:click|self={() => dispatch("cancel")}>
  <div class="consent-sheet" role="dialog" aria-modal="true" aria-label="Capability consent">
    <h2>Allow “{sidecarId}” to…</h2>
    <p class="lead">
      This sidecar runs as its own process and only gets what you allow. Choose what it may
      do — you can change this later.
    </p>

    <ul class="caps">
      {#each state.requested as cap (cap)}
        <li class:sensitive={CAPABILITY_INFO[cap].sensitive}>
          <label>
            <input type="checkbox" bind:checked={checked[cap]} />
            <span class="cap-text">
              <span class="cap-label">
                {CAPABILITY_INFO[cap].label}
                {#if CAPABILITY_INFO[cap].sensitive}<span class="badge">sensitive</span>{/if}
                {#if state.granted.includes(cap)}<span class="badge granted">granted</span>{/if}
              </span>
              <span class="cap-desc">{CAPABILITY_INFO[cap].description}</span>
            </span>
          </label>
        </li>
      {/each}
    </ul>

    <div class="actions">
      <button class="secondary" on:click={() => dispatch("cancel")}>Cancel</button>
      <button class="primary" on:click={decide}>Allow selected & continue</button>
    </div>
  </div>
</div>

<style>
  .consent-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .consent-sheet {
    width: min(520px, 92vw);
    max-height: 86vh;
    overflow: auto;
    background: var(--surface, #1e1e1e);
    color: var(--text, #eaeaea);
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 10px;
    padding: 20px 22px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }
  h2 {
    margin: 0 0 6px;
    font-size: 17px;
  }
  .lead {
    margin: 0 0 14px;
    color: var(--text-dim, #a0a0a0);
    font-size: 13px;
    line-height: 1.4;
  }
  .caps {
    list-style: none;
    margin: 0 0 18px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .caps li {
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 8px;
    padding: 10px 12px;
    background: var(--bg, #171717);
  }
  .caps li.sensitive {
    border-color: var(--warn, #b5872b);
  }
  label {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    cursor: pointer;
  }
  input[type="checkbox"] {
    margin-top: 2px;
  }
  .cap-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .cap-label {
    font-weight: 600;
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .cap-desc {
    color: var(--text-dim, #a0a0a0);
    font-size: 12px;
    line-height: 1.35;
  }
  .badge {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--warn, #b5872b);
    color: #fff;
  }
  .badge.granted {
    background: var(--accent, #3a7d3a);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  button {
    padding: 7px 14px;
    border-radius: 7px;
    border: 1px solid var(--border, #3a3a3a);
    font-size: 13px;
    cursor: pointer;
  }
  button.secondary {
    background: transparent;
    color: var(--text, #eaeaea);
  }
  button.primary {
    background: var(--accent, #2f6fed);
    border-color: var(--accent, #2f6fed);
    color: #fff;
  }
</style>
