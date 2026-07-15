<script lang="ts">
  // Repositories browser (CPE-434/435): connect to a forge (GitHub first) and browse a repo's tree
  // in-app. Backed by the host-brokered, allow-listed `forge_browse` command (no SSRF). Public repos
  // need no token; a token unlocks private ones. This is the visible surface of the forge epic.
  import { createEventDispatcher } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";

  interface RepoEntry { name: string; path: string; is_dir: boolean; size: number }

  export let provider = "github";
  /** Prefill (e.g. from a saved connection): `owner/name`. */
  export let repo = "";

  const dispatch = createEventDispatcher<{ close: void }>();

  let token = "";
  let path = "";
  let entries: RepoEntry[] = [];
  let loading = false;
  let error = "";
  let loaded = false;

  async function browse(toPath = ""): Promise<void> {
    const r = repo.trim().replace(/^https?:\/\/github\.com\//i, "").replace(/\.git$/, "");
    if (!r.includes("/")) { error = "Enter a repository as owner/name."; return; }
    repo = r;
    loading = true; error = "";
    try {
      entries = await invoke<RepoEntry[]>("forge_browse", {
        provider, repo: r, path: toPath, token: token.trim() || null,
      });
      path = toPath;
      loaded = true;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      entries = [];
    } finally {
      loading = false;
    }
  }

  let cloning = false;
  let cloneMsg = "";

  /** Clone the current repo into a user-chosen folder (CPE-436) via the host `forge_clone` command
      (hardened git args, allow-listed host). Clones into `<chosen>/<repo-name>`. */
  async function clone(): Promise<void> {
    const r = repo.trim();
    if (!r.includes("/")) { error = "Enter a repository as owner/name."; return; }
    const dir = await openFolderDialog({ directory: true, title: `Clone ${r} into which folder?` });
    if (!dir || typeof dir !== "string") return;
    const name = r.split("/").pop();
    const target = dir.replace(/[\\/]$/, "") + "/" + name;
    cloning = true; cloneMsg = `Cloning ${r} → ${target}…`; error = "";
    try {
      await invoke("forge_clone", { provider, repo: r, targetDir: target, token: token.trim() || null });
      cloneMsg = `Cloned to ${target}`;
    } catch (e) {
      cloneMsg = "";
      error = "Clone failed: " + (e instanceof Error ? e.message : String(e));
    } finally {
      cloning = false;
    }
  }

  /** Into a folder, or up one level. */
  function open(entry: RepoEntry) { if (entry.is_dir) browse(entry.path); }
  function up() {
    const parent = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
    browse(parent);
  }

  const fmtSize = (n: number) => (n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1048576).toFixed(1)} MB`);
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="repo-overlay" on:click={(e) => { if (e.target === e.currentTarget) dispatch("close"); }}>
  <div class="repo-panel">
    <div class="repo-head">
      <span><Icon name="code" size={15} /> Repositories</span>
      <button class="repo-x" title="Close" on:click={() => dispatch("close")}>×</button>
    </div>

    <div class="repo-form">
      <select bind:value={provider} title="Forge provider" aria-label="Provider">
        <option value="github">GitHub</option>
        <option value="gitlab">GitLab</option>
        <option value="bitbucket">Bitbucket</option>
        <option value="codeberg">Codeberg</option>
      </select>
      <input
        class="repo-input"
        placeholder="owner/name  (e.g. tauri-apps/tauri)"
        bind:value={repo}
        on:keydown={(e) => e.key === "Enter" && browse("")}
      />
      <input class="repo-token" type="password" placeholder="token (optional — for private repos)" bind:value={token} />
      <button class="repo-go" on:click={() => browse("")} disabled={loading}>Browse</button>
      <button class="repo-go" on:click={clone} disabled={cloning} title="Clone this repo to a local folder">
        {cloning ? "Cloning…" : "Clone"}
      </button>
    </div>

    {#if cloneMsg}<div class="repo-clonemsg">{cloneMsg}</div>{/if}

    {#if loaded && !error}
      <div class="repo-crumbs">
        <button class="repo-crumb" on:click={() => browse("")}>{repo}</button>
        {#if path}<span>/ {path}</span>{/if}
      </div>
    {/if}

    <div class="repo-list">
      {#if loading}
        <div class="repo-empty">Loading…</div>
      {:else if error}
        <div class="repo-error">{error}</div>
      {:else if loaded && entries.length === 0}
        <div class="repo-empty">This folder is empty.</div>
      {:else if loaded}
        {#if path}
          <button class="repo-row" on:click={up}><Icon name="folder" size={16} /> <span>..</span></button>
        {/if}
        {#each entries as e (e.path)}
          <button class="repo-row" class:dir={e.is_dir} on:click={() => open(e)} title={e.path}>
            <Icon name={e.is_dir ? "folder" : "file"} size={16} />
            <span class="repo-name">{e.name}</span>
            {#if !e.is_dir}<span class="repo-size">{fmtSize(e.size)}</span>{/if}
          </button>
        {/each}
      {:else}
        <div class="repo-empty">Enter a repository above and click Browse to explore it in-app.</div>
      {/if}
    </div>
  </div>
</div>

<style>
  .repo-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.45); display: flex;
    align-items: center; justify-content: center; z-index: 60; }
  .repo-panel { width: min(680px, 92vw); max-height: 82vh; display: flex; flex-direction: column;
    background: var(--bg, #1e1e1e); color: var(--fg, #eee); border: 1px solid var(--line, #444);
    border-radius: 8px; box-shadow: 0 12px 40px rgba(0,0,0,0.5); overflow: hidden; }
  .repo-head { display: flex; align-items: center; justify-content: space-between; padding: 10px 14px;
    border-bottom: 1px solid var(--line, #444); font-weight: 600; }
  .repo-x { border: 0; background: transparent; color: inherit; font-size: 20px; cursor: pointer; line-height: 1; }
  .repo-form { display: flex; gap: 6px; padding: 10px 14px; flex-wrap: wrap; }
  .repo-input { flex: 1 1 220px; min-width: 0; }
  .repo-token { flex: 1 1 160px; min-width: 0; }
  .repo-form input, .repo-form select { padding: 5px 8px; background: var(--input-bg, #2a2a2a);
    color: inherit; border: 1px solid var(--line, #555); border-radius: 4px; }
  .repo-go { padding: 5px 14px; cursor: pointer; }
  .repo-crumbs { padding: 4px 14px; font-size: 12px; opacity: 0.75; }
  .repo-crumb { border: 0; background: transparent; color: var(--accent, #6ab0ff); cursor: pointer; padding: 0; }
  .repo-list { overflow-y: auto; padding: 4px 8px 10px; }
  .repo-row { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left;
    border: 0; background: transparent; color: inherit; cursor: pointer; padding: 6px 8px; border-radius: 4px; }
  .repo-row:hover { background: rgba(128,128,128,0.16); }
  .repo-row.dir .repo-name { font-weight: 600; }
  .repo-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .repo-size { font-size: 11px; opacity: 0.55; }
  .repo-empty { padding: 20px 14px; opacity: 0.6; text-align: center; }
  .repo-error { padding: 14px; color: #e0706b; }
  .repo-clonemsg { padding: 4px 14px; font-size: 12px; color: var(--accent, #6ab0ff); }
</style>
