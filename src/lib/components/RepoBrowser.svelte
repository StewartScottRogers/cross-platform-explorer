<script lang="ts">
  // Repositories browser (CPE-434/435): connect to a forge (GitHub first) and browse a repo's tree
  // in-app. Backed by the host-brokered, allow-listed `forge_browse` command (no SSRF). Public repos
  // need no token; a token unlocks private ones. This is the visible surface of the forge epic.
  // CPE-484: restyled to match the Agent Deck launcher — labeled toolbar, unified status line, header
  // + status bar — a polished mini-app rather than a bare form.
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "../invoke";
  import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import { withBusy } from "../busy";

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
      entries = await withBusy(() => invoke<RepoEntry[]>("forge_browse", {
        provider, repo: r, path: toPath, token: token.trim() || null,
      }));
      path = toPath;
      loaded = true;
      syncToken(); // a successful browse means the token (if any) works — persist per Remember
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      entries = [];
    } finally {
      loading = false;
    }
  }

  let cloning = false;
  let cloneMsg = "";
  let remember = false;

  // Load any saved token for this provider (CPE-439) so browse/clone don't need it re-typed.
  async function loadToken(): Promise<void> {
    try {
      const saved = await invoke<string | null>("forge_get_token", { provider });
      if (saved) { token = saved; remember = true; }
    } catch { /* no host / no saved token — leave the field blank */ }
  }
  onMount(loadToken);

  /** Persist or forget the token per the Remember checkbox (CPE-439). Best-effort. */
  async function syncToken(): Promise<void> {
    try {
      if (remember && token.trim()) await invoke("forge_set_token", { provider, token: token.trim() });
      else if (!remember) await invoke("forge_delete_token", { provider });
    } catch { /* keychain unavailable — ignore */ }
  }

  /** Clone the current repo into a user-chosen folder (CPE-436) via the host `forge_clone` command
      (hardened git args, allow-listed host). Clones into `<chosen>/<repo-name>`. */
  async function clone(): Promise<void> {
    if (isGeneric) return cloneGeneric();
    const r = repo.trim();
    if (!r.includes("/")) { error = "Enter a repository as owner/name."; return; }
    const dir = await openFolderDialog({ directory: true, title: `Clone ${r} into which folder?` });
    if (!dir || typeof dir !== "string") return;
    const name = r.split("/").pop();
    const target = dir.replace(/[\\/]$/, "") + "/" + name;
    cloning = true; cloneMsg = `Cloning ${r} → ${target}…`; error = "";
    try {
      await withBusy(() => invoke("forge_clone", { provider, repo: r, targetDir: target, token: token.trim() || null }));
      cloneMsg = `Cloned to ${target}`;
    } catch (e) {
      cloneMsg = "";
      error = "Clone failed: " + (e instanceof Error ? e.message : String(e));
    } finally {
      cloning = false;
    }
  }

  // --- Generic Git provider (CPE-498): clone/sync ANY https/ssh URL, incl. self-hosted -----------
  $: isGeneric = provider === "generic";

  /** A clone held pending the user's explicit consent to reach a not-yet-admitted host. Cloning from
      a self-hosted / unknown host is gated on admitting exactly that host (no wildcard). */
  let consent: { host: string; url: string; target: string; token: string | null } | null = null;

  /** Last path segment of a git URL → the default clone folder name. */
  function repoNameFromUrl(url: string): string {
    const seg = url.trim().replace(/\.git$/i, "").replace(/[\\/]+$/, "").split(/[\\/:]/).pop();
    return seg || "repo";
  }

  /** Clone via the Generic-Git path: parse the URL host-side, pick a target folder, then clone —
      pausing for consent first if the host isn't yet admitted to the egress allow-list. */
  async function cloneGeneric(): Promise<void> {
    const url = repo.trim();
    if (!url) { error = "Enter a git URL (https://… or git@host:…)."; return; }
    error = ""; cloneMsg = "";
    let info: { host: string; scheme: string; url: string; admitted: boolean };
    try {
      info = await invoke("forge_generic_remote", { url });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      return;
    }
    const dir = await openFolderDialog({ directory: true, title: "Clone into which folder?" });
    if (!dir || typeof dir !== "string") return;
    const target = dir.replace(/[\\/]$/, "") + "/" + repoNameFromUrl(info.url);
    // A token only applies to https (ssh uses the agent/keys).
    const tok = info.scheme === "https" && token.trim() ? token.trim() : null;
    if (!info.admitted) {
      consent = { host: info.host, url, target, token: tok }; // gate egress on explicit consent
      return;
    }
    await runGenericClone(info.host, url, target, tok);
  }

  /** Admit the pending host (explicit consent given), then clone. */
  async function grantAndClone(): Promise<void> {
    if (!consent) return;
    const c = consent;
    consent = null;
    try {
      await invoke("forge_admit_host", { host: c.host });
    } catch (e) {
      error = "Couldn't grant access: " + (e instanceof Error ? e.message : String(e));
      return;
    }
    await runGenericClone(c.host, c.url, c.target, c.token);
  }

  async function runGenericClone(host: string, url: string, target: string, tok: string | null): Promise<void> {
    cloning = true; cloneMsg = `Cloning → ${target}…`; error = "";
    try {
      await withBusy(() => invoke("forge_clone_url", { url, targetDir: target, token: tok }));
      cloneMsg = `Cloned to ${target}`;
      // Per-connection credential: remember the token keyed by host (CPE-439/498). Best-effort.
      try {
        if (remember && tok) await invoke("forge_set_token", { provider: host, token: tok });
      } catch { /* keychain unavailable */ }
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

  // The single status line, mirroring the Agent Deck's #msg: errors win, then clone progress, then
  // loading, then a resting hint.
  $: statusText = error
    ? error
    : cloneMsg
      ? cloneMsg
      : loading
        ? "Loading…"
        : loaded && !isGeneric
          ? `${entries.length} item${entries.length === 1 ? "" : "s"}`
          : isGeneric
            ? "Enter any git URL and Clone. In-app browse isn't available for a generic remote — clone, then sync locally."
            : "Pick a provider, enter owner/name, then Browse.";
  $: statusKind = error ? "error" : cloneMsg ? "ok" : "";
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="repo-overlay" on:click={(e) => { if (e.target === e.currentTarget) dispatch("close"); }}>
  <div class="repo-panel">
    <div class="repo-titlebar">
      <span class="repo-title"><Icon name="code" size={15} /> Repositories</span>
      <button class="repo-x" title="Close" aria-label="Close" on:click={() => dispatch("close")}>×</button>
    </div>

    <div class="repo-toolbar">
      <div class="repo-row">
        <label class="repo-field">
          <span>Provider</span>
          <select bind:value={provider} aria-label="Provider">
            <option value="github">GitHub</option>
            <option value="gitlab">GitLab</option>
            <option value="bitbucket">Bitbucket</option>
            <option value="codeberg">Codeberg</option>
            <option value="generic">Generic Git (any URL)</option>
          </select>
        </label>
        <label class="repo-field grow">
          <span>{isGeneric ? "Git URL" : "Repository"}</span>
          <input
            placeholder={isGeneric ? "https://host/owner/repo.git  or  git@host:owner/repo.git" : "owner/name  (e.g. tauri-apps/tauri)"}
            bind:value={repo}
            spellcheck="false"
            on:keydown={(e) => e.key === "Enter" && (isGeneric ? clone() : browse(""))}
          />
        </label>
        <label class="repo-field">
          <span>Token <em>({isGeneric ? "https only" : "private repos"})</em></span>
          <input type="password" placeholder="token — optional" bind:value={token} />
        </label>
        {#if !isGeneric}
          <button class="repo-btn primary" on:click={() => browse("")} disabled={loading}>
            {loading ? "Browsing…" : "Browse"}
          </button>
        {/if}
        <button class="repo-btn" class:primary={isGeneric} on:click={clone} disabled={cloning} title="Clone this repository to a local folder">
          {cloning ? "Cloning…" : "Clone"}
        </button>
      </div>
      {#if !isGeneric}
        <label class="repo-remember" title="Save this token in the OS keychain for next time">
          <input type="checkbox" bind:checked={remember} on:change={syncToken} /> Remember token for {provider}
        </label>
      {:else}
        <label class="repo-remember" title="Save this token in the OS keychain, keyed by the host">
          <input type="checkbox" bind:checked={remember} /> Remember token for this host
        </label>
      {/if}
    </div>

    <div class="repo-status" class:error={statusKind === "error"} class:ok={statusKind === "ok"}>{statusText}</div>

    {#if consent}
      <div class="repo-consent">
        <div class="repo-consent-text">
          Allow this app to connect to <b>{consent.host}</b>? Cloning from a self-hosted or unknown host
          needs your explicit consent — only this exact host is added to the allow-list (no wildcard).
        </div>
        <div class="repo-consent-actions">
          <button class="repo-btn" on:click={() => (consent = null)}>Cancel</button>
          <button class="repo-btn primary" on:click={grantAndClone}>Grant &amp; clone</button>
        </div>
      </div>
    {/if}

    {#if loaded && !error}
      <div class="repo-crumbs">
        <button class="repo-crumb" on:click={() => browse("")}>{repo}</button>
        {#if path}<span class="repo-crumb-sep">/</span><span class="repo-crumb-cur">{path}</span>{/if}
      </div>
    {/if}

    <div class="repo-list">
      {#if isGeneric}
        <div class="repo-empty repo-generic-hint">
          <Icon name="code" size={28} />
          <p><b>Generic Git</b> clones any HTTPS or SSH remote — including self-hosted forges.</p>
          <p>Paste a URL above and click <b>Clone</b>. The first time you reach a new host you'll be
            asked to grant access to it. Once cloned, use <b>Pull / Push / Sync…</b> in the status bar.</p>
        </div>
      {:else if loading}
        <div class="repo-empty">Loading…</div>
      {:else if error}
        <!-- The full error is shown in the status line above; keep the body generic so it isn't duplicated. -->
        <div class="repo-empty repo-err">Couldn't load — see the message above.</div>
      {:else if loaded && entries.length === 0}
        <div class="repo-empty">This folder is empty.</div>
      {:else if loaded}
        {#if path}
          <button class="repo-row-item" on:click={up}><Icon name="folder" size={16} /> <span class="repo-name">..</span></button>
        {/if}
        {#each entries as e (e.path)}
          <button class="repo-row-item" class:dir={e.is_dir} on:click={() => open(e)} title={e.path}>
            <Icon name={e.is_dir ? "folder" : "file"} size={16} />
            <span class="repo-name">{e.name}</span>
            {#if !e.is_dir}<span class="repo-size">{fmtSize(e.size)}</span>{/if}
          </button>
        {/each}
      {:else}
        <div class="repo-empty">Enter a repository above and click <b>Browse</b> to explore it in-app.</div>
      {/if}
    </div>

    <div class="repo-statusbar">
      <span class="repo-sb-repo">{loaded ? repo : "No repository open"}</span>
      {#if loaded && path}<span class="repo-sb-path">/ {path}</span>{/if}
    </div>
  </div>
</div>

<style>
  .repo-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.45); display: flex;
    align-items: center; justify-content: center; z-index: 60; }
  /* A polished mini-app window matching the Agent Deck launcher: header · toolbar · status · body ·
     status bar. Uses the explorer theme tokens (app.css) so it is legible on the light theme. */
  .repo-panel { width: min(760px, 94vw); height: min(620px, 88vh); display: flex; flex-direction: column;
    background: var(--surface); color: var(--text); border: 1px solid var(--border-strong);
    border-radius: 8px; box-shadow: 0 16px 48px rgba(0,0,0,0.4); overflow: hidden; }

  .repo-titlebar { display: flex; align-items: center; justify-content: space-between;
    padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .repo-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .repo-x { border: 0; background: transparent; color: var(--text-dim); font-size: 20px; cursor: pointer;
    line-height: 1; padding: 0 4px; border-radius: 4px; }
  .repo-x:hover { background: rgba(128,128,128,0.18); color: var(--text); }

  /* Toolbar — mirrors the Agent Deck: subtle-grey band, labeled fields, 30px rounded controls. */
  .repo-toolbar { padding: 11px 14px; display: flex; flex-direction: column; gap: 9px;
    background: var(--surface-alt); border-bottom: 1px solid var(--border); }
  .repo-row { display: flex; gap: 12px; align-items: flex-end; }
  .repo-field { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .repo-field.grow { flex: 1; }
  .repo-field > span { font-size: 10px; text-transform: uppercase; letter-spacing: .06em;
    opacity: .55; font-weight: 600; }
  .repo-field > span em { font-style: normal; text-transform: none; letter-spacing: 0; opacity: .8; }
  .repo-field select, .repo-field input { font: inherit; height: 30px; padding: 0 9px;
    background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: 6px; }
  .repo-field select:focus, .repo-field input:focus { outline: none; border-color: var(--accent); }
  .repo-field select { min-width: 130px; }
  .repo-field input[type="password"] { width: 150px; }

  .repo-btn { font: inherit; height: 30px; padding: 0 14px; border-radius: 6px; cursor: pointer;
    border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); font-weight: 500; }
  .repo-btn:hover:not(:disabled) { background: rgba(128,128,128,0.12); }
  .repo-btn.primary { height: 30px; min-width: 96px; background: var(--accent); border-color: var(--accent);
    color: #fff; font-weight: 600; }
  .repo-btn.primary:hover:not(:disabled) { filter: brightness(1.08); }
  .repo-btn:disabled { opacity: .5; cursor: default; }

  .repo-remember { display: flex; align-items: center; gap: 6px; font-size: 12px; opacity: .8;
    white-space: nowrap; }

  /* Generic-Git host consent (CPE-498): a distinct band naming the host before any egress. */
  .repo-consent { display: flex; align-items: center; gap: 14px; padding: 10px 14px;
    background: var(--surface-alt); border-bottom: 1px solid var(--border-strong); }
  .repo-consent-text { flex: 1; font-size: 12px; line-height: 1.45; }
  .repo-consent-actions { display: flex; gap: 8px; flex: 0 0 auto; }

  .repo-generic-hint { display: flex; flex-direction: column; align-items: center; gap: 8px;
    max-width: 460px; margin: 0 auto; line-height: 1.5; }
  .repo-generic-hint p { margin: 0; }

  /* Status line — the launcher's #msg equivalent. */
  .repo-status { padding: 6px 14px; font-size: 12px; min-height: 20px; opacity: .85;
    border-bottom: 1px solid var(--border); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .repo-status.error { color: #e0706b; opacity: 1; }
  .repo-status.ok { color: var(--accent); opacity: 1; }

  .repo-crumbs { display: flex; align-items: center; gap: 4px; padding: 6px 14px; font-size: 12px;
    border-bottom: 1px solid var(--border); overflow-x: auto; white-space: nowrap; }
  .repo-crumb { border: 0; background: transparent; color: var(--accent); cursor: pointer; padding: 0;
    font: inherit; }
  .repo-crumb-sep { opacity: .5; }
  .repo-crumb-cur { opacity: .8; }

  .repo-list { flex: 1; overflow-y: auto; padding: 4px 8px; }
  .repo-row-item { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left;
    border: 0; background: transparent; color: inherit; cursor: pointer; padding: 6px 8px; border-radius: 4px; }
  .repo-row-item:hover { background: rgba(128,128,128,0.16); }
  .repo-row-item.dir .repo-name { font-weight: 600; }
  .repo-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .repo-size { font-size: 11px; opacity: 0.55; }
  .repo-empty { padding: 28px 14px; opacity: 0.6; text-align: center; }
  .repo-err { color: #e0706b; opacity: 1; }

  .repo-statusbar { display: flex; align-items: center; gap: 8px; height: 26px; padding: 0 14px;
    background: var(--surface-alt); border-top: 1px solid var(--border); font-size: 12px; color: var(--text-dim); }
  .repo-sb-repo { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .repo-sb-path { opacity: .7; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
