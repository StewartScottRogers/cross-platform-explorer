<script lang="ts">
  /**
   * Generic labelled-text-input dialog for a macro's `{ask:label}` prompt-parameters (CPE-1190 UI
   * half, epic CPE-739). Reuses `PasswordPromptDialog`'s shape (backdrop/dialog/Escape-cancels), but
   * generalized from one masked field to N plain labelled fields — one per distinct `{ask:label}`
   * token the macro references (see `macroParams.extractAskLabels`). Submitting dispatches a
   * `label -> value` map (an unanswered field submits as `""`, matching the backend's "absent param
   * resolves to nothing" rule via `macroParams.resolveAskParams`); the run flow (CPE-1191) is the only
   * caller and does the actual substitution + dry-run + execute.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";

  export let title = "Macro parameters";
  export let message = "This macro asks for a few values before it runs:";
  /** The distinct `{ask:label}` labels to prompt for, in the order they first appear in the macro. */
  export let labels: string[] = [];

  const dispatch = createEventDispatcher<{ submit: Record<string, string>; cancel: void }>();

  let values: Record<string, string> = {};
  $: for (const label of labels) if (!(label in values)) values[label] = "";

  let fieldsEl: HTMLDivElement;

  onMount(async () => {
    await tick();
    fieldsEl?.querySelector<HTMLInputElement>("input")?.focus();
  });

  function submit() {
    // Always emit every requested label (default "" for one left untouched) so the caller never has
    // to guess which labels were actually asked for.
    const out: Record<string, string> = {};
    for (const label of labels) out[label] = values[label] ?? "";
    dispatch("submit", out);
  }

  function onKeydown(e: KeyboardEvent) {
    // Only Enter here — Escape is handled once, globally, by the svelte:window listener below
    // (matching PasswordPromptDialog's note on why: a field's keydown bubbles to window too, so
    // handling Escape in both places would double-dispatch cancel).
    if (e.key === "Enter") submit();
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={title} on:click|stopPropagation>
    <h2>{title}</h2>
    {#if message}<p>{message}</p>{/if}

    <div class="fields" bind:this={fieldsEl}>
      {#if labels.length === 0}
        <div class="empty" data-testid="no-params">Nothing to ask for.</div>
      {/if}
      {#each labels as label (label)}
        <div class="field">
          <label for="macro-param-{label}">{label}</label>
          <input
            id="macro-param-{label}"
            bind:value={values[label]}
            on:keydown={onKeydown}
            data-testid="param-field-{label}"
            spellcheck="false"
          />
        </div>
      {/each}
    </div>

    <div class="actions">
      <button class="btn" data-testid="cancel-btn" on:click={() => dispatch("cancel")}>Cancel</button>
      <button class="btn primary" data-testid="ok-btn" on:click={submit}>Continue</button>
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
    width: 420px;
    max-width: 90vw;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 10px; }
  p { color: var(--text-dim); margin-bottom: 14px; line-height: 1.5; font-size: 12.5px; }
  .fields { display: flex; flex-direction: column; gap: 10px; }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-faint);
    font-weight: 600;
  }
  .field input {
    height: 32px;
    padding: 0 8px;
    font: inherit;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-sizing: border-box;
  }
  .field input:focus { outline: none; border-color: var(--accent); }
  .empty { color: var(--text-dim); font-size: 12.5px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .btn.primary:hover { background: var(--accent-hover); }
</style>
