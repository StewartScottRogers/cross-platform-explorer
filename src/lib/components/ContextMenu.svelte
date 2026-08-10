<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import Submenu from "./Submenu.svelte";
  import { t } from "../i18n";
  import type { ViewMode, SortKey, SortDir } from "../types";
  import { NEW_FILE_TYPE_GROUPS } from "../newFileTypes";

  export let x = 0;
  export let y = 0;
  /** "item" when opened on a row, "empty" when opened on blank space, "drive" when opened on a
   *  disk/drive tile or sidebar drive row (CPE-1158) — a focused folder-like menu whose actions all
   *  target the drive's ROOT path, deliberately omitting rename/delete/cut/etc. that make no sense for
   *  a whole volume. */
  export let target: "item" | "empty" | "drive" | "home-item" = "item";
  export let canPaste = false;
  export let selectionCount = 0;
  /** True when exactly one folder is selected — enables "Open in new tab". */
  export let folderSelected = false;
  /** True when exactly one executable file is selected — enables Execute (CPE-241). */
  export let executableSelected = false;
  /** Icon for the "Open" item — the selected entry's own icon (CPE-243). */
  export let openIcon = "folder";
  /** Whether the selected folder is pinned to Home — toggles Pin/Unpin (CPE-249). */
  export let pinned = false;
  /** Whether the single selected item is a favorite — toggles the label (CPE-338). */
  export let favorited = false;
  /** True when the selection can be packed into a .zip (CPE-251). */
  export let compressible = false;
  /** True when exactly two files (no folders) are selected — enables Compare (CPE-418). */
  export let comparable = false;
  /** True when exactly one archive file is selected — enables Extract (CPE-252). */
  export let extractable = false;
  /** True when exactly one ZIP-family archive file is selected — enables "Check archive safety…"
   *  (CPE-1318, epic CPE-1002). Narrower than {@link extractable}: the backend scan only scores true ZIP
   *  containers (see `ARCHIVE_SAFETY_EXTS` in `archiveExts.ts`), not tar/gz/7z/iso. */
  export let archiveSafetyEligible = false;
  /** True when Open-in-Terminal applies (a real folder, not Home/archive) (CPE-253). */
  export let canTerminal = false;
  /** True when "Copy to…"/"Move to…" apply (CPE-355, CPE-1384): a real writable location for either
   *  pane — pane A's `!isHome && !archive`, or pane B's `paneBPath !== HOME` (pane B is always a plain
   *  real folder in v1). Deliberately its own prop, not `canTerminal` — `canTerminal` also gates
   *  terminal/console/compress rows that stay pane-A-only, but copy/move-to-folder is now pane-routed. */
  export let copyMoveEligible = false;
  /** Extension (no dot) to offer "Select all .ext"; empty hides the row (CPE-258). */
  export let sameTypeExt = "";
  /** True when the selection contains at least one image file (of 2+ selected) — enables "Batch media…"
   *  (CPE-1093, pane-routed by CPE-1384). The dialog itself pre-filters any non-image files out of the
   *  eligible set. */
  export let mediaEligible = false;
  /** Current view mode — the empty-area View ▸ submenu checkmarks it (CPE-1153). Single source of
   *  truth: the same `view` state the toolbar/CommandBar drive; selecting here dispatches `view:<mode>`. */
  export let view: ViewMode = "details";
  /** Current sort key + direction — Sort by ▸ checkmarks them; selecting dispatches `sort:<key>` /
   *  `sortdir:<dir>` into the same state the column headers use (CPE-1153). */
  export let sortKey: SortKey = "name";
  export let sortDir: SortDir = "asc";
  /** Home-item menu (CPE-1162, `target: "home-item"`): which segmented list the right-clicked row came
   *  from, so the correct view-native "Remove from <view>" (+ "Clear all" on Recent) shows and the
   *  cross-view Add-to-Favorites/Pin rows appear only where they make sense. */
  export let homeView: "recent" | "favorites" | "folders" | "shared" | "" = "";
  /** For a Shared row (CPE-1163): its kind ("mapped" | "mount" | "user"), so the menu offers Disconnect
   *  (a mapped drive) vs Remove (a user-added location). Empty for the non-shared views. */
  export let homeKind = "";
  /** Whether the right-clicked Home row is a folder — toggles Open-in-new-tab / New ▸ / the Open icon. */
  export let homeIsDir = false;
  /** True when a best-effort existence check found the row's real target missing (CPE-1162). Disables
   *  the on-disk actions (Open/Reveal/Copy/Rename/Delete/New/Properties + Add-to-Fav/Pin) but KEEPS the
   *  pointer-level "Remove from <view>" / "Clear all" enabled so a dead entry can still be pruned. */
  export let homeStale = false;
  /** Whether the undo stack has anything to undo — gates the empty-area Undo row (CPE-1153). */
  export let canUndo = false;
  /** Human label of the top undo entry (e.g. "Rename to report.txt"); appended after "Undo" when set. */
  export let undoLabel = "";
  /** Saved macro names bound to the "context" surface (CPE-1191, epic CPE-739) — each dispatches
   *  `macro:<name>` for App.svelte to run over the current selection via the dry-run confirm flow.
   *  Empty hides the "Run macro ▸" submenu entirely (no macros bound, or none saved yet). */
  export let macros: string[] = [];
  /** User-defined commands bound to the "context" surface (CPE-783, epic CPE-711; wired CPE-1577) —
   *  each dispatches `uc:<id>` for App.svelte to resolve + run (through the same confirm-before-launch
   *  `RunCommandConfirm` gate the Palette surface already uses). Empty hides the "Run command ▸"
   *  submenu entirely (no commands bound to Context, or none defined yet). */
  export let userCommands: { id: string; name: string }[] = [];
  /** True when the single selected item (`target: "item"`) is a broken symlink (CPE-1209, epic CPE-715)
   *  — determined via `commands.linkStatus(...).broken` by the opener (App.svelte), since it needs an
   *  async check the menu itself can't do. Shows "Repair link…" only for that one case. */
  export let linkBroken = false;
  /** True when every selected item is a plain file (no folders), we're on a real writable location
   *  (not Home/archive), and at least one item is selected — enables "Securely delete…" (CPE-1240, epic
   *  CPE-738). The shred engine overwrites+removes a single file; it isn't recursive, so folders are
   *  excluded rather than silently skipped mid-menu. */
  export let shreddable = false;
  /** True when a SINGLE folder is selected in a real filesystem location (not Home/archive) — enables
   *  "Create encrypted vault…" (CPE-1250, epic CPE-738), which seals that folder into a `.cpevault`. */
  export let vaultable = false;
  /** Kind of the single selected cert/CSR-family file (CPE-1424, epic CPE-1417): `"csr"` for a `.csr`
   *  file enables "Issue cert from this CSR…"; `"cert"` for a `.pem`/`.crt`/`.cer`/`.der` file enables
   *  "Sign with this as CA…" instead. Either value also enables the shared "Inspect" row. `""` hides all
   *  three — narrower than the preview pane's own cert-family detection (no `.pub`/`.key`, which aren't
   *  csr/cert-shaped enough to sign/issue from). */
  export let certFileKind: "csr" | "cert" | "" = "";
  /** True when the single selected item is a `.jwt`/`.jws` file (CPE-1424) — enables "Inspect JWT". */
  export let jwtSelected = false;
  /** True on a real writable location for either pane (CPE-1424) — pane A's `!isHome && !archive`, or
   *  pane B's `paneBPath !== HOME`, same eligibility shape as {@link copyMoveEligible}. Enables "Create
   *  certificate here…" on a folder row (combined with {@link folderSelected}) and on the empty-area
   *  menu. */
  export let certCreateEligible = false;
  /** True when the right-clicked drive (`target: "drive"`) is REMOVABLE (CPE-1278) — adds a "Safely
   *  eject" item to the drive menu. Fixed/system/network drives never set this, so they can't be ejected
   *  from the menu any more than from the sidebar's inline button. */
  export let driveEjectable = false;
  /** True when a SINGLE, non-empty, regular file is selected (CPE-1509, parent CPE-1491) — enables
   *  "Split file…", which opens `SplitFileDialog`. */
  export let splitEligible = false;
  /** True when a SINGLE regular file is selected that's either a `.split-manifest.json` or one of its
   *  numbered `.NNN` parts (CPE-1509) — enables "Join parts…", which opens `JoinPartsDialog`. */
  export let joinEligible = false;

  const dispatch = createEventDispatcher<{
    action: string;
    close: void;
  }>();

  let el: HTMLDivElement;
  let left = x;
  let top = y;

  // CPE-1160 — kill the open/close-race footgun by construction. The `contextmenu` event that
  // OPENS this menu (dispatched from a row/tile/pane handler) keeps bubbling up the DOM to the
  // window dismisser below, in the SAME tick as this component mounts. Historically that closed
  // the just-opened menu ~5 ms later unless the opener remembered `e.stopPropagation()` — a trap
  // that bit three times (CPE-1154/1157/1159). Instead of relying on every opener to remember,
  // we record when the menu opened (this component is created fresh per-open via `{#if ctx}` in
  // App, so this timestamp IS the open time) and have the window dismisser IGNORE any
  // `contextmenu` that arrives within a tiny threshold of open — i.e. the opening event itself.
  // A LATER right-click elsewhere (well past the threshold) still closes/repositions; left-clicks
  // are never gated (a click never opens the menu), so click-outside dismissal stays instant.
  const openedAt = performance.now();
  const OPEN_GUARD_MS = 50;

  function onWindowContextmenu(e: MouseEvent) {
    e.preventDefault();
    // Ignore the opening event (same tick as mount); a genuine later right-click still closes.
    if (performance.now() - openedAt < OPEN_GUARD_MS) return;
    dispatch("close");
  }

  // Keep the menu on screen — a menu that opens half off the edge is useless.
  onMount(() => {
    const rect = el.getBoundingClientRect();
    const pad = 6;
    left = Math.min(x, window.innerWidth - rect.width - pad);
    top = Math.min(y, window.innerHeight - rect.height - pad);
    left = Math.max(pad, left);
    top = Math.max(pad, top);
    el.focus();
  });

  function run(action: string) {
    dispatch("action", action);
    dispatch("close");
  }
</script>

<svelte:window
  on:click={() => dispatch("close")}
  on:contextmenu={onWindowContextmenu}
  on:keydown={(e) => e.key === "Escape" && dispatch("close")}
/>

<!-- svelte-ignore a11y-no-noninteractive-element-interactions a11y-click-events-have-key-events -->
<div
  class="ctx"
  role="menu"
  tabindex="-1"
  bind:this={el}
  style="left:{left}px; top:{top}px"
  on:click|stopPropagation
  on:contextmenu|stopPropagation|preventDefault
>
  {#if target === "item"}
    <!-- Win11 puts the common actions in a quick-action icon row at the top. -->
    <div class="quickrow">
      <button title="Cut (Ctrl+X)" on:click={() => run("cut")}><Icon name="cut" /></button>
      <button title="Copy (Ctrl+C)" on:click={() => run("copy")}><Icon name="copy" /></button>
      <button
        title={selectionCount > 1 ? "Rename one item at a time" : "Rename (F2)"}
        disabled={selectionCount !== 1}
        on:click={() => run("rename")}><Icon name="rename" /></button>
      <button title="Delete (Del)" on:click={() => run("delete")}><Icon name="delete" /></button>
    </div>
    <div class="sep" />
    <button class="row" role="menuitem" on:click={() => run("open")}>
      <Icon name={openIcon} size={15} /> {$t('ctx.open')}
    </button>
    {#if executableSelected}
      <button class="row" role="menuitem" on:click={() => run("execute")}>
        <Icon name="executable" size={15} /> {$t('ctx.execute')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("execute-admin")}>
        <Icon name="executable" size={15} /> {$t('ctx.executeAdmin')}
      </button>
    {/if}
    {#if folderSelected}
      <button class="row" role="menuitem" on:click={() => run("open-new-tab")}>
        <Icon name="plus" size={15} /> {$t('ctx.openNewTab')}
      </button>
      {#if canTerminal}
        <button class="row" role="menuitem" on:click={() => run("terminal-folder")}>
          <Icon name="code" size={15} /> {$t('ctx.openInTerminal')}
        </button>
      {/if}
    {/if}
    {#if canTerminal}
      <button class="row" role="menuitem" on:click={() => run("open-in-console")}>
        <Icon name="code" size={15} /> {$t('ctx.workOnThis')}
      </button>
    {/if}
    {#if linkBroken}
      <!-- Repair link… (CPE-1209, epic CPE-715): only offered for a broken symlink — the opener
           (App.svelte) resolves `linkBroken` via `commands.linkStatus` before showing this menu. -->
      <button class="row" role="menuitem" on:click={() => run("repair-link")}>
        <Icon name="link" size={15} /> {$t('ctx.repairLink')}
      </button>
    {/if}
    <div class="sep" />
    <!-- New ▸ on the on-item menu (CPE-1156) — the same CPE-1153 submenu as the empty-area menu, so
         right-clicking ANY item still exposes New. When a single FOLDER is selected the create targets
         INSIDE it (`new-*-in`, handled with `selectedEntries[0].path`); on a file / multi-selection it
         creates in the current folder (`new-*`), same as the empty-area menu. -->
    <Submenu label={$t('cmd.new')} icon="plus">
      <button class="row" role="menuitem" on:click={() => run(folderSelected ? "new-folder-in" : "new-folder")}>
        <Icon name="folder" size={15} /> {$t('ctx.folder')}
        {#if !folderSelected}<span class="hint">Ctrl+Shift+N</span>{/if}
      </button>
      <button class="row" role="menuitem" on:click={() => run(folderSelected ? "new-file-in" : "new-file")}>
        <Icon name="document" size={15} /> {$t('ctx.textFile')}
      </button>
      {#each NEW_FILE_TYPE_GROUPS as group, gi}
        {#if gi > 0}<div class="sep" role="separator" />{/if}
        {#each group as ft}
          <button class="row" role="menuitem" on:click={() => run(`${folderSelected ? 'new-file-in' : 'new-file'}:${ft.ext}`)}>
            <Icon name={ft.icon} size={15} /> {$t(ft.labelKey)}
          </button>
        {/each}
      {/each}
    </Submenu>
    <div class="sep" />
    <button class="row" role="menuitem" on:click={() => run("duplicate")}>
      <Icon name="copy" size={15} /> {$t('ctx.duplicate')}
      <span class="hint">Ctrl+D</span>
    </button>
    <button class="row" role="menuitem" on:click={() => run("copy-path")}>
      <Icon name="paste" size={15} /> {$t('ctx.copyAsPath')}
      <span class="hint">Ctrl+Shift+C</span>
    </button>
    {#if copyMoveEligible}
      <button class="row" role="menuitem" on:click={() => run("copy-to")}>
        <Icon name="copy" size={15} /> {$t('ctx.copyToFolder')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("move-to")}>
        <Icon name="cut" size={15} /> {$t('ctx.moveToFolder')}
      </button>
    {/if}
    <button class="row" role="menuitem" on:click={() => run("copy-name")}>
      <Icon name="rename" size={15} /> {$t('ctx.copyName')}
    </button>
    <!-- Add to Drop Stack (CPE-1531, epic CPE-1489): shelves the selection into the persistent
         cross-folder stack (CPE-1530's store) for a later move/copy-all (CPE-1533). Always offered on
         the item menu — same as Duplicate/Copy as path/Copy name above — since this menu only opens
         with a selection; `doAddToDropStack` (App.svelte) applies the same archive gate as Copy/Cut. -->
    <button class="row" role="menuitem" on:click={() => run("add-drop-stack")}>
      <Icon name="layers" size={15} /> Add to Drop Stack
      <span class="hint">Ctrl+Shift+D</span>
    </button>
    {#if selectionCount > 1}
      <button class="row" role="menuitem" on:click={() => run("batch-rename")}>
        <Icon name="rename" size={15} /> {$t('ctx.rename')}
      </button>
    {/if}
    {#if selectionCount > 1 && mediaEligible}
      <button class="row" role="menuitem" on:click={() => run("batch-media")}>
        <Icon name="image" size={15} /> {$t('ctx.batchMedia')}
      </button>
    {/if}
    {#if comparable}
      <button class="row" role="menuitem" on:click={() => run("compare")}>
        <Icon name="copy" size={15} /> {$t('ctx.compareFiles')}
      </button>
    {/if}
    {#if sameTypeExt}
      <button class="row" role="menuitem" on:click={() => run("select-type")}>
        <Icon name="filter" size={15} /> {$t('ctx.selectAllExt', { ext: sameTypeExt })}
      </button>
    {/if}
    {#if extractable}
      <button class="row" role="menuitem" on:click={() => run("extract")}>
        <Icon name="archive" size={15} /> {$t('ctx.extract')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("extract-to")}>
        <Icon name="archive" size={15} /> {$t('ctx.extractTo')}
      </button>
    {/if}
    {#if archiveSafetyEligible}
      <!-- Check archive safety… (CPE-1318, epic CPE-1002): surfaces the zip-bomb / compression-ratio
           report for a ZIP-family archive. Leading icon + theme-var text (MENUS.md); the DANGER
           treatment lives in the dialog, not this menu row. -->
      <button class="row" role="menuitem" on:click={() => run("archive-safety")}>
        <Icon name="archive" size={15} /> {$t('ctx.archiveSafety')}
      </button>
    {/if}
    {#if splitEligible}
      <!-- Split file… (CPE-1509, parent CPE-1491): chunks a non-empty regular file into fixed-size
           numbered parts + a small manifest. Leading icon + theme-var text (MENUS.md). -->
      <button class="row" role="menuitem" on:click={() => run("split-file")}>
        <Icon name="cut" size={15} /> Split file…
      </button>
    {/if}
    {#if joinEligible}
      <!-- Join parts… (CPE-1509): rejoins a manifest's numbered parts back into the original file. -->
      <button class="row" role="menuitem" on:click={() => run("join-parts")}>
        <Icon name="link" size={15} /> Join parts…
      </button>
    {/if}
    {#if compressible}
      <button class="row" role="menuitem" on:click={() => run("compress")}>
        <Icon name="archive" size={15} /> {$t('ctx.compressZip')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("compress-targz")}>
        <Icon name="archive" size={15} /> {$t('ctx.compressTarGz')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("compress-password")}>
        <Icon name="lock" size={15} /> {$t('ctx.compressWithPassword')}
      </button>
    {/if}
    {#if vaultable}
      <!-- Create encrypted vault… (CPE-1250, epic CPE-738): seal a single folder into a `.cpevault`.
           Leading icon + theme-var text, never red (MENUS.md); the create dialog (not this menu) carries
           the passphrase entry + the honest destructive warning for the optional shred-original path. -->
      <button class="row" role="menuitem" on:click={() => run("vault-create")}>
        <Icon name="lock" size={15} /> Create encrypted vault…
      </button>
    {/if}
    {#if certFileKind === "csr"}
      <!-- Issue cert from this CSR… (CPE-1424, epic CPE-1417): opens SignCertDialog pre-filled with the
           clicked .csr as the CSR to issue from. Leading icon + theme-var text (MENUS.md). -->
      <button class="row" role="menuitem" on:click={() => run("cert-issue-from-csr")}>
        <Icon name="certificate" size={15} /> Issue cert from this CSR…
      </button>
    {:else if certFileKind === "cert"}
      <!-- Sign with this as CA… (CPE-1424): opens SignCertDialog pre-filled with the clicked cert file
           as the CA certificate to sign with. -->
      <button class="row" role="menuitem" on:click={() => run("cert-sign-as-ca")}>
        <Icon name="certificate" size={15} /> Sign with this as CA…
      </button>
    {/if}
    {#if certFileKind}
      <!-- Inspect (CPE-1424): the row is already selected (right-click selects first), and the preview
           pane auto-decodes a cert/CSR file on selection — this just brings the preview tab forward. -->
      <button class="row" role="menuitem" on:click={() => run("cert-inspect")}>
        <Icon name="certificate" size={15} /> Inspect
      </button>
    {/if}
    {#if jwtSelected}
      <!-- Inspect JWT (CPE-1424): same reasoning as Inspect above, for a .jwt/.jws file. -->
      <button class="row" role="menuitem" on:click={() => run("jwt-inspect")}>
        <Icon name="lock" size={15} /> Inspect JWT
      </button>
    {/if}
    {#if certCreateEligible && folderSelected}
      <!-- Create certificate here… (CPE-1424): opens CreateCertDialog defaulted to write into the
           clicked folder. -->
      <button class="row" role="menuitem" on:click={() => run("cert-create-here")}>
        <Icon name="certificate" size={15} /> Create certificate here…
      </button>
    {/if}
    {#if folderSelected}
      <button class="row" role="menuitem" on:click={() => run("pin")}>
        <Icon name="pin" size={15} /> {pinned ? $t('ctx.unpinFromHome') : $t('ctx.pinToHome')}
      </button>
    {/if}
    {#if selectionCount === 1}
      <button class="row" role="menuitem" on:click={() => run("favorite")}>
        <Icon name="star" size={15} /> {favorited ? $t('ctx.removeFavorite') : $t('ctx.addFavorite')}
      </button>
    {/if}
    <!-- Tags apply to a single item or a whole multi-selection (batch add, CPE-656). -->
    <button class="row" role="menuitem" on:click={() => run("tags")}>
      <Icon name="tag" size={15} /> {$t('ctx.tags')}
    </button>
    {#if macros.length > 0}
      <!-- Saved macros bound to this surface (CPE-1191). Each runs App's dry-run confirm flow over
           the current selection — the same "each" selection model as user commands. -->
      <Submenu label="Run macro" icon="function">
        {#each macros as name}
          <button class="row" role="menuitem" on:click={() => run(`macro:${name}`)}>
            <Icon name="function" size={15} /> {name}
          </button>
        {/each}
      </Submenu>
    {/if}
    {#if userCommands.length > 0}
      <!-- User-defined commands bound to this surface (CPE-783, wired CPE-1577). Each runs App's
           confirm-before-launch flow (RunCommandConfirm) over the current selection — same gate as the
           Palette surface. -->
      <Submenu label="Run command" icon="code">
        {#each userCommands as c (c.id)}
          <button class="row" role="menuitem" on:click={() => run(`uc:${c.id}`)}>
            <Icon name="code" size={15} /> {c.name}
          </button>
        {/each}
      </Submenu>
    {/if}
    <div class="sep" />
    <button class="row" role="menuitem" on:click={() => run("reveal")}>
      <Icon name="folder" size={15} /> {$t('ctx.reveal')}
    </button>
    <button class="row" role="menuitem" on:click={() => run("properties")}>
      <Icon name="info" size={15} /> {$t('ctx.properties')}
      <span class="hint">Alt+Enter</span>
    </button>
    <button class="row" role="menuitem" on:click={() => run("metadataStudio")}>
      <Icon name="info" size={15} /> {$t('studio.menu')}
    </button>
    {#if shreddable}
      <!-- Securely delete… (CPE-1240, epic CPE-738): kept in its own separated group, same convention
           as the Home-item destructive group (CPE-1162) — a rare, irreversible action stays visually
           apart from the routine ones above. Leading icon, theme-var text, never red (MENUS.md); the
           confirm dialog (not this menu) carries the honest permanence + platform-caveat messaging. -->
      <div class="sep" />
      <button class="row" role="menuitem" on:click={() => run("shred")}>
        <Icon name="delete" size={15} /> {$t('ctx.shred')}
      </button>
    {/if}
    <div class="sep" />
    <button class="row" role="menuitem" on:click={() => run("help-docs")}>
      <Icon name="book" size={15} /> Documents for this view
    </button>
  {:else if target === "drive"}
    <!-- Drive/disk menu (CPE-1158): a folder-like menu targeting the drive's ROOT path. Open navigates
         into the root; New ▸ creates AT the root (reusing CPE-1156's create-in-target path); Properties
         is for the root. Rename/Delete/Cut/etc. are intentionally absent — you don't rename a volume. -->
    <button class="row" role="menuitem" on:click={() => run("drive-open")}>
      <Icon name="drive" size={15} /> {$t('ctx.open')}
    </button>
    <div class="sep" role="separator" />
    <Submenu label={$t('cmd.new')} icon="plus">
      <button class="row" role="menuitem" on:click={() => run("drive-new-folder")}>
        <Icon name="folder" size={15} /> {$t('ctx.folder')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("drive-new-file")}>
        <Icon name="document" size={15} /> {$t('ctx.textFile')}
      </button>
      {#each NEW_FILE_TYPE_GROUPS as group, gi}
        {#if gi > 0}<div class="sep" role="separator" />{/if}
        {#each group as ft}
          <button class="row" role="menuitem" on:click={() => run(`drive-new-file:${ft.ext}`)}>
            <Icon name={ft.icon} size={15} /> {$t(ft.labelKey)}
          </button>
        {/each}
      {/each}
    </Submenu>
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" on:click={() => run("drive-copy-path")}>
      <Icon name="paste" size={15} /> {$t('ctx.copyAsPath')}
    </button>
    <button class="row" role="menuitem" on:click={() => run("drive-terminal")}>
      <Icon name="code" size={15} /> {$t('ctx.openInTerminal')}
    </button>
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" on:click={() => run("drive-properties")}>
      <Icon name="info" size={15} /> {$t('ctx.properties')}
    </button>
    {#if driveEjectable}
      <div class="sep" role="separator" />
      <button class="row" role="menuitem" on:click={() => run("drive-eject")}>
        <Icon name="eject" size={15} /> {$t('ctx.ejectDrive')}
      </button>
    {/if}
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" on:click={() => run("help-docs")}>
      <Icon name="book" size={15} /> Documents for this view
    </button>
  {:else if target === "home-item"}
    <!-- Home Recent/Favorites/Folders ROW menu (CPE-1162). Every on-disk action targets the row's real
         path (stored in App, independent of any FileList selection — Home has none). The CRITICAL
         peculiarity: the DESTRUCTIVE "Delete" (trashes the real file, top group) is kept unmistakably
         separate — distinct wording, distinct group, a separator between — from the pointer-level
         "Remove from <view>" (prunes only the list ENTRY, list-management group at the bottom). Stale
         targets disable the on-disk rows but keep Remove/Clear enabled. MENUS.md: theme vars, leading
         icons, never red text. -->
    {#if homeView === "shared"}
      <!-- Shared (network / mapped / SMB) row menu (CPE-1163). Reuses the CPE-1162 machinery via the
           new `view: "shared"`. An unreachable share degrades gracefully: Open surfaces its own error,
           while Disconnect/Remove stay live regardless (there is no stale-disable here — see App's
           onHomeItemContext, which skips the stat check for shares). Disconnect vs Remove is chosen by
           `homeKind`. MENUS.md: theme vars, leading icons, never red text. -->
      <button class="row" role="menuitem" on:click={() => run("home-open")}>
        <Icon name="folder" size={15} /> {$t('ctx.open')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("home-open-new-tab")}>
        <Icon name="plus" size={15} /> {$t('ctx.openNewTab')}
      </button>
      <div class="sep" role="separator" />
      <button class="row" role="menuitem" on:click={() => run("home-copy-path")}>
        <Icon name="paste" size={15} /> {$t('ctx.copyAsPath')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("home-properties")}>
        <Icon name="info" size={15} /> {$t('ctx.properties')}
      </button>
      <div class="sep" role="separator" />
      {#if homeKind === "user"}
        <button class="row" role="menuitem" on:click={() => run("share-remove")}>
          <Icon name="close" size={15} /> {$t('home.removeNetworkLocation')}
        </button>
      {:else if homeKind === "mapped"}
        <button class="row" role="menuitem" on:click={() => run("share-disconnect")}>
          <Icon name="close" size={15} /> {$t('home.disconnectShare')}
        </button>
      {/if}
      <div class="sep" role="separator" />
      <button class="row" role="menuitem" on:click={() => run("help-docs")}>
        <Icon name="book" size={15} /> Documents for this view
      </button>
    {:else}
    <button class="row" role="menuitem" disabled={homeStale} on:click={() => run("home-open")}>
      <Icon name={homeIsDir ? "folder" : "document"} size={15} /> {$t('ctx.open')}
    </button>
    {#if homeIsDir}
      <button class="row" role="menuitem" disabled={homeStale} on:click={() => run("home-open-new-tab")}>
        <Icon name="plus" size={15} /> {$t('ctx.openNewTab')}
      </button>
    {/if}
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" disabled={homeStale} on:click={() => run("home-reveal")}>
      <Icon name="folder" size={15} /> {$t('ctx.reveal')}
    </button>
    <button class="row" role="menuitem" disabled={homeStale} on:click={() => run("home-copy")}>
      <Icon name="copy" size={15} /> {$t('ctx.copy')}
    </button>
    <button class="row" role="menuitem" disabled={homeStale} on:click={() => run("home-copy-path")}>
      <Icon name="paste" size={15} /> {$t('ctx.copyAsPath')}
    </button>
    <button class="row" role="menuitem" disabled={homeStale} on:click={() => run("home-rename")}>
      <Icon name="rename" size={15} /> {$t('ctx.rename')}
    </button>
    {#if homeIsDir && !homeStale}
      <!-- New ▸ inside the folder (CPE-1156/1161's typed list) — home-new-* target the row's own path.
           Hidden entirely when the target is stale (you can't create inside a folder that's gone). -->
      <Submenu label={$t('cmd.new')} icon="plus">
        <button class="row" role="menuitem" on:click={() => run("home-new-folder")}>
          <Icon name="folder" size={15} /> {$t('ctx.folder')}
        </button>
        <button class="row" role="menuitem" on:click={() => run("home-new-file")}>
          <Icon name="document" size={15} /> {$t('ctx.textFile')}
        </button>
        {#each NEW_FILE_TYPE_GROUPS as group, gi}
          {#if gi > 0}<div class="sep" role="separator" />{/if}
          {#each group as ft}
            <button class="row" role="menuitem" on:click={() => run(`home-new-file:${ft.ext}`)}>
              <Icon name={ft.icon} size={15} /> {$t(ft.labelKey)}
            </button>
          {/each}
        {/each}
      </Submenu>
    {/if}
    <button class="row" role="menuitem" disabled={homeStale} on:click={() => run("home-properties")}>
      <Icon name="info" size={15} /> {$t('ctx.properties')}
    </button>
    <!-- Destructive group — the real-file trash, kept apart from the list-management "Remove" below. -->
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" disabled={homeStale} on:click={() => run("home-delete")}>
      <Icon name="delete" size={15} /> {$t('ctx.delete')}
    </button>
    <!-- Cross-view group. -->
    {#if homeView === "recent" || homeView === "folders"}
      <div class="sep" role="separator" />
      <button class="row" role="menuitem" disabled={homeStale} on:click={() => run("home-favorite")}>
        <Icon name="star" size={15} /> {$t('ctx.addFavorite')}
      </button>
      {#if homeIsDir}
        <button class="row" role="menuitem" disabled={homeStale} on:click={() => run("home-pin")}>
          <Icon name="pin" size={15} /> {$t('home.pinToQuickAccess')}
        </button>
      {/if}
    {/if}
    <!-- List-management group — prunes only the ENTRY (never the file); stays enabled even when stale. -->
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" on:click={() => run("home-remove")}>
      <Icon name="close" size={15} />
      {#if homeView === "favorites"}{$t('home.removeFromFavorites')}
      {:else if homeView === "folders"}{$t('home.removeFromRecentFolders')}
      {:else}{$t('home.removeFromRecent')}{/if}
    </button>
    {#if homeView === "recent"}
      <button class="row" role="menuitem" on:click={() => run("home-clear")}>
        <Icon name="delete" size={15} /> {$t('home.clearAll')}
      </button>
    {/if}
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" on:click={() => run("help-docs")}>
      <Icon name="book" size={15} /> Documents for this view
    </button>
    {/if}
  {:else}
    <!-- New ▸ — Windows 11 shape: create actions live behind a submenu (CPE-1153). -->
    <Submenu label={$t('cmd.new')} icon="plus">
      <button class="row" role="menuitem" on:click={() => run("new-folder")}>
        <Icon name="folder" size={15} /> {$t('ctx.folder')}
        <span class="hint">Ctrl+Shift+N</span>
      </button>
      <button class="row" role="menuitem" on:click={() => run("new-file")}>
        <Icon name="document" size={15} /> {$t('ctx.textFile')}
      </button>
      {#each NEW_FILE_TYPE_GROUPS as group, gi}
        {#if gi > 0}<div class="sep" role="separator" />{/if}
        {#each group as ft}
          <button class="row" role="menuitem" on:click={() => run(`new-file:${ft.ext}`)}>
            <Icon name={ft.icon} size={15} /> {$t(ft.labelKey)}
          </button>
        {/each}
      {/each}
      <div class="sep" role="separator" />
      <!-- New Link… (CPE-1207, epic CPE-715): opens NewLinkDialog for a symlink/hardlink, same reach as
           New folder/file above but a dialog rather than an inline-rename create (kind + target need
           picking first). -->
      <button class="row" role="menuitem" on:click={() => run("new-link")}>
        <Icon name="link" size={15} /> {$t('ctx.newLink')}
      </button>
    </Submenu>
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" disabled={!canPaste} on:click={() => run("paste")}>
      <Icon name="paste" size={15} /> {$t('ctx.paste')}
      <span class="hint">Ctrl+V</span>
    </button>
    <button class="row" role="menuitem" disabled={!canUndo} on:click={() => run("undo")}>
      <Icon name="back" size={15} /> {$t('ctx.undo')}{undoLabel ? ` ${undoLabel}` : ''}
      <span class="hint">Ctrl+Z</span>
    </button>
    <div class="sep" role="separator" />
    <!-- View ▸ — drives the SAME `view` state the toolbar/CommandBar use; current mode checkmarked. -->
    <Submenu label={$t('cmd.view')} icon="view">
      <button class="row" role="menuitemradio" aria-checked={view === 'details'} on:click={() => run("view:details")}>
        <Icon name="details" size={15} /> {$t('view.details')}
        {#if view === 'details'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={view === 'list'} on:click={() => run("view:list")}>
        <Icon name="sort" size={15} /> {$t('view.list')}
        {#if view === 'list'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={view === 'icons'} on:click={() => run("view:icons")}>
        <Icon name="view" size={15} /> {$t('view.icons')}
        {#if view === 'icons'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={view === 'gallery'} on:click={() => run("view:gallery")}>
        <Icon name="gallery" size={15} /> {$t('view.gallery')}
        {#if view === 'gallery'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
    </Submenu>
    <!-- Sort by ▸ — drives the SAME `sortKey`/`sortDir` the column headers use; both checkmarked. -->
    <Submenu label={$t('cmd.sort')} icon="sort">
      <button class="row" role="menuitemradio" aria-checked={sortKey === 'name'} on:click={() => run("sort:name")}>
        <Icon name="rename" size={15} /> {$t('sort.name')}
        {#if sortKey === 'name'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={sortKey === 'modified'} on:click={() => run("sort:modified")}>
        <Icon name="recent" size={15} /> {$t('sort.modified')}
        {#if sortKey === 'modified'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={sortKey === 'type'} on:click={() => run("sort:type")}>
        <Icon name="filter" size={15} /> {$t('sort.type')}
        {#if sortKey === 'type'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={sortKey === 'size'} on:click={() => run("sort:size")}>
        <Icon name="disk" size={15} /> {$t('sort.size')}
        {#if sortKey === 'size'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <div class="sep" role="separator" />
      <button class="row" role="menuitemradio" aria-checked={sortDir === 'asc'} on:click={() => run("sortdir:asc")}>
        <Icon name="chev-up" size={15} /> {$t('cmd.ascending')}
        {#if sortDir === 'asc'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={sortDir === 'desc'} on:click={() => run("sortdir:desc")}>
        <Icon name="chev-down" size={15} /> {$t('cmd.descending')}
        {#if sortDir === 'desc'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
    </Submenu>
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" on:click={() => run("select-all")}>
      <Icon name="check" size={15} /> {$t('ctx.selectAll')}
      <span class="hint">Ctrl+A</span>
    </button>
    <button class="row" role="menuitem" on:click={() => run("invert-selection")}>
      <Icon name="check" size={15} /> {$t('ctx.invertSelection')}
    </button>
    <button class="row" role="menuitem" on:click={() => run("select-pattern")}>
      <Icon name="filter" size={15} /> {$t('ctx.selectByPattern')}
    </button>
    <button class="row" role="menuitem" on:click={() => run("refresh")}>
      <Icon name="refresh" size={15} /> {$t('ctx.refresh')}
      <span class="hint">F5</span>
    </button>
    <div class="sep" role="separator" />
    {#if canTerminal}
      <button class="row" role="menuitem" on:click={() => run("terminal")}>
        <Icon name="code" size={15} /> {$t('ctx.openInTerminal')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("open-folder-in-console")}>
        <Icon name="code" size={15} /> {$t('ctx.workOnFolder')}
      </button>
    {/if}
    <button class="row" role="menuitem" on:click={() => run("reveal")}>
      <Icon name="folder" size={15} /> {$t('ctx.reveal')}
    </button>
    {#if canTerminal}
      <button class="row" role="menuitem" on:click={() => run("properties-folder")}>
        <Icon name="info" size={15} /> {$t('ctx.properties')}
      </button>
    {/if}
    {#if certCreateEligible}
      <!-- Create certificate here… (CPE-1424, epic CPE-1417): opens CreateCertDialog defaulted to write
           into the clicked/active pane's current folder. -->
      <div class="sep" role="separator" />
      <button class="row" role="menuitem" on:click={() => run("cert-create-here")}>
        <Icon name="certificate" size={15} /> Create certificate here…
      </button>
    {/if}
    <!-- Command Palette (CPE-1164) — a right-click affordance for the Ctrl+Shift+P palette; App maps
         the `command-palette` action to paletteOpen via runAction. Leading icon per MENUS.md. -->
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" on:click={() => run("command-palette")}>
      <Icon name="search" size={15} /> {$t('palette.ariaPalette')}
      <span class="hint">Ctrl+Shift+P</span>
    </button>
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" on:click={() => run("help-docs")}>
      <Icon name="book" size={15} /> Documents for this view
    </button>
  {/if}
</div>

<style>
  .ctx {
    position: fixed;
    z-index: 100;
    min-width: 210px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.16);
    outline: none;
  }
  .quickrow {
    display: flex;
    gap: 2px;
    padding: 2px;
  }
  .quickrow button {
    flex: 1;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    text-align: left;
    border-radius: var(--radius);
    white-space: nowrap; /* menu items are always one line — the menu grows to fit (CPE-753) */
  }
  .hint {
    margin-left: auto;
    color: var(--text-faint);
    font-size: 12px;
  }
  /* Trailing ✓ marking the active view / sort key / direction — accent, per MENUS.md. */
  .check {
    margin-left: auto;
    display: inline-flex;
    color: var(--accent);
  }
  .sep {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
</style>
