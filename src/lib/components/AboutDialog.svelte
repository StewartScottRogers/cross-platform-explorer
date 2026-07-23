<script lang="ts">
  /** About dialog (CPE-229): app name, running version, and a docs link. The
      version is passed in (read at runtime by App via getVersion), never
      hard-coded. Link clicks are delegated to App via the `openurl` event so
      URL-opening lives in one place.

      Sidecars (CPE-923): lists every registered sidecar (name, version, contract,
      health) from `sidecar_details`. Loaded here since it's display-only; returns
      `[]` on the plain (sidecar-free) build, in which case the section is hidden. */
  import { createEventDispatcher, onMount } from "svelte";
  import { sidecarDetails, type SidecarInfo } from "../sidecar";

  export let version = "";
  export let repoUrl = "";

  const dispatch = createEventDispatcher<{ close: void; openurl: string }>();

  let sidecars: SidecarInfo[] = [];
  onMount(async () => {
    sidecars = await sidecarDetails();
  });

  /** A sidecar's one-word health, worst-first (mirrors SidecarManager.statusOf). */
  function health(s: SidecarInfo): { label: string; tone: "ok" | "warn" | "bad" | "idle" } {
    if (!s.binary_ok) return { label: "missing", tone: "bad" };
    if (!s.compatible) return { label: "incompatible", tone: "bad" };
    if (!s.enabled) return { label: "disabled", tone: "idle" };
    if (s.running) return { label: "running", tone: "ok" };
    return { label: "ready", tone: "idle" };
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="About" on:click|stopPropagation>
    <h2>Cross-Platform Explorer</h2>
    <p class="ver">Version {version || "—"}</p>
    <p class="desc">A fast, cross-platform file explorer with one-click install and auto-updates.</p>

    {#if sidecars.length}
      <section class="sidecars">
        <h3>Sidecars</h3>
        <ul>
          {#each sidecars as s (s.id)}
            {@const h = health(s)}
            <li>
              <span class="sc-name" title={s.id}>{s.name}</span>
              <span class="sc-ver">v{s.version || "—"}</span>
              <span class="sc-contract" title="Contract version">contract {s.contract || "—"}</span>
              <span class="sc-status {h.tone}">{h.label}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <div class="links">
      <button class="link" on:click={() => dispatch("openurl", repoUrl)}>Documentation</button>
    </div>

    <div class="actions">
      <button class="btn primary" on:click={() => dispatch("close")}>Close</button>
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
    width: 400px;
    max-width: 90vw;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 4px; }
  .ver { color: var(--text-dim); font-size: 13px; margin-bottom: 12px; }
  .desc { color: var(--text-dim); line-height: 1.5; margin-bottom: 14px; }
  .sidecars { margin-bottom: 16px; }
  .sidecars h3 { font-size: 12px; text-transform: uppercase; letter-spacing: .04em; color: var(--text-faint); margin-bottom: 8px; }
  .sidecars ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .sidecars li { display: flex; align-items: baseline; gap: 8px; font-size: 12px; }
  .sc-name { font-weight: 600; color: var(--text); }
  .sc-ver { color: var(--text-dim); font-variant-numeric: tabular-nums; }
  .sc-contract { color: var(--text-faint); }
  .sc-status { margin-left: auto; font-size: 11px; padding: 1px 7px; border-radius: 999px;
    border: 1px solid var(--border); color: var(--text-dim); white-space: nowrap; flex: 0 0 auto; }
  .sc-status.ok { color: #3a9d4a; border-color: rgba(58,157,74,0.5); }
  .sc-status.warn { color: #c9862a; border-color: rgba(201,134,42,0.5); }
  .sc-status.bad { color: #d05656; border-color: rgba(208,86,86,0.5); }

  .links { margin-bottom: 18px; }
  .link {
    padding: 0;
    color: var(--accent);
    text-decoration: underline;
    background: transparent;
  }
  .link:hover { color: var(--accent-hover); background: transparent; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
  }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover { background: var(--accent-hover); }
</style>
