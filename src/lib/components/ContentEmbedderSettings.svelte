<script lang="ts">
  /** Settings › AI content search — configurable real embedder (CPE-1273, epic CPE-976).
   *
   * File-content search (CPE-1262/1263) defaults to a local, dependency-free embedder — no key, no
   * network. This self-contained section lets the user point it at ANY OpenAI-compatible `/embeddings`
   * endpoint instead: a LOCAL server (LM Studio / Ollama — no key) or a hosted one (OpenAI/others — with a
   * key). Off by default — the plain feature is unchanged until enabled here (never a launch-time modal,
   * [[avoid-modal-permission-popups]]).
   *
   * Self-managing like SpotlightHotkeySettings/ShellIntegration: it owns its own persistence. The
   * enabled/URL/model persist to settings.json (settings.ts); the API KEY is written ONLY to the OS
   * keychain via `content_embedder_set_key` and is NEVER echoed back or persisted in plaintext — the field
   * only ever shows a "key saved" indicator (from `content_embedder_has_key`), never the value.
   *
   * OS-gated honesty: whether a given endpoint actually embeds well is the user's own server — this UI can
   * only verify that the endpoint is reachable and reports a vector dimensionality ("Test connection",
   * `content_embedder_test`). Switching embedder/model rebuilds the per-folder index the next time it's
   * searched (the backend keys the index by embedder identity), so a switch prompts a fresh build rather
   * than serving mismatched vectors. */
  import { onMount } from "svelte";
  import { commands } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import * as settings from "../settings";

  let enabled = settings.loadContentEmbedderEnabled();
  let baseUrl = settings.loadContentEmbedderBaseUrl();
  let model = settings.loadContentEmbedderModel();

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
      hasKey = unwrap(await commands.contentEmbedderHasKey());
    } catch {
      hasKey = false; // a keychain read failure degrades to "no key" — a local server needs none anyway
    }
  });

  function setEnabled(on: boolean) {
    enabled = on;
    settings.saveContentEmbedderEnabled(on);
  }
  function applyBaseUrl() {
    baseUrl = baseUrl.trim();
    settings.saveContentEmbedderBaseUrl(baseUrl);
  }
  function applyModel() {
    model = model.trim();
    settings.saveContentEmbedderModel(model);
  }

  async function saveKey() {
    keyBusy = true;
    testMsg = "";
    try {
      // An empty field CLEARS the stored key (a local server needs none). The value is sent once and
      // never read back.
      unwrap(await commands.contentEmbedderSetKey(keyInput));
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
      const dim = unwrap(
        await commands.contentEmbedderTest({ enabled: true, base_url: baseUrl.trim(), model: model.trim() }),
      );
      testOk = true;
      testMsg = `Connected — the model returns ${dim}-dimensional vectors.`;
    } catch (e) {
      testOk = false;
      testMsg = e instanceof Error ? e.message : String(e);
    } finally {
      testBusy = false;
    }
  }
</script>

<div class="section-title">AI content search</div>
<div class="settings-row">
  <span>Use a real embeddings model for content search</span>
  <input
    type="checkbox"
    checked={enabled}
    data-testid="content-embedder-toggle"
    on:change={(e) => setEnabled(e.currentTarget.checked)}
  />
</div>
<div class="note">
  Works with any OpenAI-compatible embeddings endpoint — a local server like LM Studio needs no key;
  OpenAI/others need an API key. Off by default: content search uses a built-in local model (no key, no
  network).
</div>

<div class="settings-row">
  <span>Endpoint URL</span>
  <input
    type="text"
    class="text-input"
    placeholder="http://localhost:1234/v1"
    bind:value={baseUrl}
    data-testid="content-embedder-url"
    on:blur={applyBaseUrl}
    on:keydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
  />
</div>
<div class="settings-row">
  <span>Model</span>
  <input
    type="text"
    class="text-input"
    placeholder="text-embedding-3-small"
    bind:value={model}
    data-testid="content-embedder-model"
    on:blur={applyModel}
    on:keydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
  />
</div>
<div class="settings-row">
  <span>API key {#if hasKey}<span class="key-saved" data-testid="content-embedder-key-saved">(saved)</span>{/if}</span>
  <span class="key-row">
    <input
      type="password"
      class="text-input key"
      placeholder={hasKey ? "•••••••• (leave blank to keep)" : "not needed for a local server"}
      bind:value={keyInput}
      disabled={keyBusy}
      data-testid="content-embedder-key"
      autocomplete="off"
    />
    <button class="mini" disabled={keyBusy} data-testid="content-embedder-key-save" on:click={saveKey}>
      {keyInput.trim() ? "Save" : "Clear"}
    </button>
  </span>
</div>

<div class="settings-row">
  <button class="test-btn" disabled={testBusy} data-testid="content-embedder-test" on:click={testConnection}>
    {testBusy ? "Testing…" : "Test connection"}
  </button>
</div>
{#if testMsg}
  <div class="note" class:error={!testOk} class:ok={testOk} data-testid="content-embedder-test-msg">{testMsg}</div>
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
