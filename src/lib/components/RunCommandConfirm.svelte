<script lang="ts">
  /**
   * Confirm-before-launch for user commands (CPE-783, epic CPE-711): shows the exact command line(s) that
   * will run as EXTERNAL PROCESSES and requires an explicit Run click before invoking the backend
   * `run_command`. This is the safety gate for the user-command feature — nothing spawns without it. After
   * running, it shows each command's exit code + captured stdout/stderr.
   */
  import { createEventDispatcher } from "svelte";
  import { unwrap } from "../invoke";
  // Aliased: this component already has a `commands` prop (the command lines to run).
  import { commands as api } from "../bindings.gen"; // typed client (CPE-964)
  import Icon from "./Icon.svelte";

  /** The command's display name. */
  export let title = "";
  /** The resolved command line(s) to run (from userCommands.resolveCommand). */
  export let commands: string[] = [];
  /** Working directory to run in ("" ⇒ the backend default). */
  export let cwd = "";

  const dispatch = createEventDispatcher<{ close: void }>();

  type CmdOut = { stdout: string; stderr: string; code: number | null; truncated: boolean };
  type Result = CmdOut & { command: string; error?: string };

  let running = false;
  let results: Result[] | null = null;

  async function run() {
    running = true;
    const out: Result[] = [];
    for (const command of commands) {
      try {
        const r = unwrap(await api.runCommand(command, cwd || null));
        out.push({ command, ...r });
      } catch (e) {
        out.push({ command, stdout: "", stderr: "", code: null, truncated: false, error: String(e) });
      }
    }
    results = out;
    running = false;
  }

  const failed = (r: Result) => !!r.error || (r.code ?? 0) !== 0;
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && !running && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => !running && dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <header>
      <Icon name="code" size={15} />
      <h2>Run “{title}”?</h2>
      <button class="x" title="Close" on:click={() => dispatch("close")} disabled={running}><Icon name="close" size={14} /></button>
    </header>

    {#if !results}
      <p class="warn">
        This runs <b>{commands.length}</b> external {commands.length === 1 ? "command" : "commands"} on your
        machine{cwd ? ` in ${cwd}` : ""}. Review before running:
      </p>
      <ul class="cmds">
        {#each commands as c}<li>{c}</li>{/each}
        {#if commands.length === 0}<li class="dim">Nothing to run for the current selection.</li>{/if}
      </ul>
      <div class="actions">
        <button class="btn" on:click={() => dispatch("close")} disabled={running}>Cancel</button>
        <button class="btn primary" on:click={run} disabled={running || commands.length === 0}>{running ? "Running…" : "Run"}</button>
      </div>
    {:else}
      <div class="results">
        {#each results as r}
          <div class="res" class:err={failed(r)}>
            <div class="res-cmd">{r.command}</div>
            {#if r.error}
              <div class="res-out err-text">✗ {r.error}</div>
            {:else}
              <div class="res-meta">exit {r.code ?? "signal"}{r.truncated ? " · output truncated" : ""}</div>
              {#if r.stdout}<pre class="res-out">{r.stdout}</pre>{/if}
              {#if r.stderr}<pre class="res-out err-text">{r.stderr}</pre>{/if}
            {/if}
          </div>
        {/each}
      </div>
      <div class="actions"><button class="btn primary" on:click={() => dispatch("close")}>Close</button></div>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.3); display: grid; place-items: center; z-index: 210; }
  .dialog {
    width: 620px; max-width: 94vw; max-height: 84vh; display: flex; flex-direction: column;
    background: var(--surface); color: var(--text); border: 1px solid var(--border-strong);
    border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.28); padding: 14px 18px 16px;
  }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }
  h2 { font-size: 15px; flex: 1; }
  .x { width: 28px; height: 28px; display: grid; place-items: center; color: var(--text-dim); }
  .x:hover { color: var(--text); }
  .warn { font-size: 13px; color: var(--text-dim); line-height: 1.5; margin-bottom: 8px; }
  .cmds { list-style: none; margin: 0 0 12px; padding: 0; display: flex; flex-direction: column; gap: 4px; overflow: auto; }
  .cmds li {
    font-family: ui-monospace, monospace; font-size: 12.5px; padding: 6px 9px; border-radius: 6px;
    background: var(--surface-alt); border: 1px solid var(--border); white-space: pre-wrap; word-break: break-all;
  }
  .cmds li.dim { color: var(--text-faint); font-family: inherit; }
  .results { overflow: auto; display: flex; flex-direction: column; gap: 8px; margin-bottom: 12px; }
  .res { border: 1px solid var(--border); border-radius: 6px; padding: 8px 10px; }
  .res.err { border-color: rgba(208, 86, 86, 0.5); }
  .res-cmd { font-family: ui-monospace, monospace; font-size: 12px; font-weight: 600; margin-bottom: 3px; word-break: break-all; }
  .res-meta { font-size: 11px; color: var(--text-dim); margin-bottom: 4px; }
  .res-out { font-family: ui-monospace, monospace; font-size: 12px; white-space: pre-wrap; margin: 0; max-height: 160px; overflow: auto; }
  .err-text { color: #d05656; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: auto; }
  .btn { height: 32px; padding: 0 16px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); }
  .btn:disabled { opacity: 0.5; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
