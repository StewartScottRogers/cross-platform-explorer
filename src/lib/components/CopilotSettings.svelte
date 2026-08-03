<script lang="ts">
  /** Settings › AI file copilot — the model-endpoint config for the copilot (CPE-1276, epic CPE-977).
   *
   * The AI file copilot (CPE-1275) turns a natural-language instruction ("archive the old screenshots")
   * into a SAFE, whitelisted file-operation plan the user previews and confirms before anything runs. It
   * needs a configured OpenAI-compatible chat endpoint to work at all — this section is that config. Off
   * by default: with `enabled` false (or the endpoint/model left blank), `copilot_plan` refuses to produce
   * a plan, so nothing changes about the app until the user opts in here.
   *
   * Mirrors ContentEmbedderSettings.svelte (CPE-1273) byte-for-byte in shape: self-contained (owns its own
   * persistence via settings.ts), enabled/URL/model persist to settings.json, and the API KEY is written
   * ONLY to the OS keychain via `copilot_set_key` — never echoed back or persisted in plaintext. The field
   * only ever shows a "key saved" indicator (from `copilot_has_key`), never the value.
   *
   * Honest copy: works with any OpenAI-compatible CHAT endpoint (not embeddings) — a local server like LM
   * Studio needs no key; OpenAI/others need an API key. "Test connection" only verifies the endpoint
   * answers with a parseable plan — it says nothing about how good that model's plans actually are. */
  import { onMount } from "svelte";
  import { commands } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import * as settings from "../settings";

  let enabled = settings.loadCopilotEnabled();
  let baseUrl = settings.loadCopilotBaseUrl();
  let model = settings.loadCopilotModel();

  // The API key is write-only from the UI's perspective: we track only WHETHER one is saved, never its
  // value. `keyInput` is a scratch field the user types into to Save/replace; it's cleared after saving.
  let hasKey = false;
  let keyInput = "";
  let keyBusy = false;

  let testBusy = false;
  let testMsg = "";
  let testOk = false;

  onMount(async () => {
    try {
      hasKey = unwrap(await commands.copilotHasKey());
    } catch {
      hasKey = false; // a keychain read failure degrades to "no key" — a local server needs none anyway
    }
  });

  function setEnabled(on: boolean) {
    enabled = on;
    settings.saveCopilotEnabled(on);
  }
  function applyBaseUrl() {
    baseUrl = baseUrl.trim();
    settings.saveCopilotBaseUrl(baseUrl);
  }
  function applyModel() {
    model = model.trim();
    settings.saveCopilotModel(model);
  }

  async function saveKey() {
    keyBusy = true;
    testMsg = "";
    try {
      // An empty field CLEARS the stored key (a local server needs none). The value is sent once and
      // never read back.
      unwrap(await commands.copilotSetKey(keyInput));
      hasKey = keyInput.trim().length > 0;
      keyInput = "";
    } catch (e) {
      testOk = false;
      testMsg = `Couldn't save the key: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      keyBusy = false;
    }
  }

  async function testConnection() {
    testBusy = true;
    testMsg = "";
    testOk = false;
    try {
      unwrap(await commands.copilotTest({ enabled: true, base_url: baseUrl.trim(), model: model.trim() }));
      testOk = true;
      testMsg = "Connected — the model returned a parseable plan.";
    } catch (e) {
      testOk = false;
      testMsg = e instanceof Error ? e.message : String(e);
    } finally {
      testBusy = false;
    }
  }
</script>

<div class="section-title">AI file copilot</div>
<div class="settings-row">
  <span>Enable the AI file copilot</span>
  <input
    type="checkbox"
    checked={enabled}
    data-testid="copilot-toggle"
    on:change={(e) => setEnabled(e.currentTarget.checked)}
  />
</div>
<div class="note">
  Type a plain-language instruction for a folder ("organize these by file type") and the copilot proposes
  a plan — moves, renames, deletes, new folders, copies — that you review and confirm before anything runs.
  Works with any OpenAI-compatible CHAT endpoint — a local server like LM Studio needs no key; OpenAI/others
  need an API key. Off by default: no plan can be produced until you enable this and set an endpoint below.
</div>

<div class="settings-row">
  <span>Endpoint URL</span>
  <input
    type="text"
    class="text-input"
    placeholder="http://localhost:1234/v1"
    bind:value={baseUrl}
    data-testid="copilot-url"
    on:blur={applyBaseUrl}
    on:keydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
  />
</div>
<div class="settings-row">
  <span>Model</span>
  <input
    type="text"
    class="text-input"
    placeholder="gpt-4o-mini"
    bind:value={model}
    data-testid="copilot-model"
    on:blur={applyModel}
    on:keydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
  />
</div>
<div class="settings-row">
  <span>API key {#if hasKey}<span class="key-saved" data-testid="copilot-key-saved">(saved)</span>{/if}</span>
  <span class="key-row">
    <input
      type="password"
      class="text-input key"
      placeholder={hasKey ? "•••••••• (leave blank to keep)" : "not needed for a local server"}
      bind:value={keyInput}
      disabled={keyBusy}
      data-testid="copilot-key"
      autocomplete="off"
    />
    <button class="mini" disabled={keyBusy} data-testid="copilot-key-save" on:click={saveKey}>
      {keyInput.trim() ? "Save" : "Clear"}
    </button>
  </span>
</div>

<div class="settings-row">
  <button class="test-btn" disabled={testBusy} data-testid="copilot-test" on:click={testConnection}>
    {testBusy ? "Testing…" : "Test connection"}
  </button>
</div>
{#if testMsg}
  <div class="note" class:error={!testOk} class:ok={testOk} data-testid="copilot-test-msg">{testMsg}</div>
{/if}

<style>
  .section-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    margin: 16px 0 6px;
  }
  .settings-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 0;
  }
  .text-input {
    width: 220px;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
    font-size: 12px;
  }
  .key-row { display: flex; gap: 6px; align-items: center; }
  .text-input.key { width: 168px; }
  .mini {
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
    font-size: 12px;
    flex: 0 0 auto;
  }
  .mini:disabled { opacity: 0.5; }
  .key-saved { color: var(--text-faint); font-weight: 400; }
  .test-btn {
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
    font-size: 12px;
  }
  .test-btn:disabled { opacity: 0.5; }
  .note {
    font-size: 12px;
    color: var(--text-dim);
    margin-top: 2px;
  }
  .note.error { color: var(--danger); }
  .note.ok { color: var(--text); }
</style>
