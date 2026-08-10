// Section → documentation mapping (CPE-595): the ONE source of truth for "which doc page documents this
// section", driving both the contextual Help open (CPE-596) and the exhaustiveness guard test. Don't
// scatter doc slugs across components — add the section here and everything else follows.
//
// Self-maintaining (CPE-597): the guard test asserts every `Section` has a mapping and every mapped slug
// exists in `DOCS`, so a new section without its doc — or a typo'd/renamed slug — fails CI.

import { DOCS } from "./docs";

/** Every user-facing surface that has contextual help. Add a section id here when it earns a doc page. */
export type Section =
  | "home"
  | "explorer"
  | "disk-usage"
  | "ai-console"
  | "agent-grid"
  | "agent-board"
  | "workbench"
  | "repositories"
  | "swarms"
  | "native-metadata"
  | "terminal"
  | "vaults"
  | "file-health"
  | "crypto-preview"
  | "folder-preview"
  | "cert-management"
  | "media-player"
  | "structured-previews"
  | "network"
  | "split-join"
  | "compare"
  | "drop-stack";

/** Section id → doc slug (a `src/docs/*.md` filename without `.md`). */
const SECTION_DOC: Record<Section, string> = {
  home: "01-overview",
  explorer: "03-explorer",
  "disk-usage": "11-disk-usage",
  "ai-console": "04-ai-console",
  "agent-grid": "05-agent-grid",
  "agent-board": "06-agent-board",
  workbench: "07-workbench",
  repositories: "08-repositories",
  swarms: "09-swarms",
  // CPE-1178 (epic CPE-717): the native tag/metadata bridge — Settings' opt-in toggle (CPE-1177) plus
  // the Properties "Native metadata" section and tag-editor Pull/Push controls it reveals (CPE-1176).
  // Not a standalone view switched into via the sidebar (there's no dedicated screen for it, unlike
  // the other entries above) — it's a cross-cutting feature reachable from Settings/Properties/the tag
  // editor — but it still earns its own doc page + registry entry per [[maintain-in-app-docs-library]]
  // so contextual help has somewhere exact to point.
  "native-metadata": "17-native-metadata",
  // CPE-1243 (epic CPE-714): the embedded terminal dock — like native-metadata above, not a sidebar view
  // you switch into (it's a panel toggled over the explorer via the CommandBar's Terminal button), but it
  // still earns its own doc page + registry entry per [[maintain-in-app-docs-library]].
  terminal: "19-terminal",
  // CPE-1250 (epic CPE-738): encrypted vaults — the "Create encrypted vault…" folder action + the
  // unlock/browse/lock flow (CPE-1249) + the keychain Settings toggle. Like native-metadata/terminal
  // above, not a sidebar view you switch into (it's a cross-cutting feature reachable from a folder's
  // context menu, a `.cpevault` file, and Settings), but it still earns its own doc page + registry
  // entry per [[maintain-in-app-docs-library]].
  vaults: "20-vaults",
  // CPE-1315 (epic CPE-1002): the File Health panel — a tabbed dialog surfacing the file-inspection
  // detectors (dangling/cyclic symlinks this slice, more categories in later slices). Like vaults/
  // terminal/native-metadata above, not a sidebar view you switch into (it's reachable from the Tools
  // menu / command palette), but it still earns its own doc page + registry entry per
  // [[maintain-in-app-docs-library]].
  "file-health": "22-file-health",
  // CPE-1422 (epic CPE-1417): the JWT/certificate preview-pane views (CPE-1418/1419's decoders wired
  // into the pane). Like file-health/vaults/terminal/native-metadata above, not a sidebar view you
  // switch into (it's whichever decoded panel the preview pane shows for a .jwt/.pem/etc. file), but it
  // still earns its own doc page + registry entry per [[maintain-in-app-docs-library]].
  "crypto-preview": "26-crypto-preview",
  // CPE-1426: the preview pane's folder-peek browser (`FolderBrowser.svelte`) — selecting a folder in
  // the main list shows its contents one level down in the preview pane, and clicking a subfolder
  // there drives the main pane's navigate + select so a tree can be walked from the preview pane alone.
  // Like crypto-preview/file-health/vaults above, not a sidebar view you switch into (it's just what the
  // preview pane shows for a highlighted directory), but it still earns its own doc page + registry
  // entry per [[maintain-in-app-docs-library]].
  "folder-preview": "27-folder-preview",
  // CPE-1423/1424 (epic CPE-1417): the CREATE/SIGN side of certificate management — CreateCertDialog +
  // SignCertDialog, opened from the pane-aware context menu's "Create certificate here…" / "Issue cert
  // from this CSR…" / "Sign with this as CA…" or the command palette. Like crypto-preview/vaults above,
  // not a sidebar view you switch into (it's a pair of dialogs reachable from a folder/cert-file's
  // context menu and the palette), but it still earns its own doc page + registry entry per
  // [[maintain-in-app-docs-library]].
  "cert-management": "28-cert-management",
  // CPE-1429 (epic CPE-720): the preview pane's audio/video player + custom transport. Like
  // folder-preview/crypto-preview above, not a sidebar view you switch into (it's what the preview pane
  // shows for a selected media file), but it still earns its own doc page + registry entry per
  // [[maintain-in-app-docs-library]].
  "media-player": "29-media-player",
  // CPE-1434 (epic CPE-1433): the structured-preview cards (the `.eml` email viewer this slice; more
  // formats in later children). Like crypto-preview/media-player above, not a sidebar view you switch
  // into (it's whichever decoded panel the preview pane shows for a supported file), but it still earns
  // its own doc page + registry entry per [[maintain-in-app-docs-library]].
  "structured-previews": "30-structured-previews",
  // CPE-1513 (epic CPE-1498): the Network sidebar section — saved SFTP/WebDAV connections + OS-discovered
  // shares. Like folder-preview/media-player above, not a full-screen view you switch into (it's a
  // sidebar section + its add/edit/connect popovers), but it still earns its own doc page + registry
  // entry per [[maintain-in-app-docs-library]].
  network: "31-network",
  // CPE-1509 (parent CPE-1491): the Split file… / Join parts… dialogs + their context-menu entries. Like
  // network/media-player above, not a sidebar view you switch into (it's a pair of dialogs reachable from
  // a file's context menu), but it still earns its own doc page + registry entry per
  // [[maintain-in-app-docs-library]].
  "split-join": "33-split-join",
  // CPE-1508 (epic CPE-722, parent CPE-1490): the Compare dialog (CPE-779's folder/text/byte compare,
  // CPE-1508's image compare). Like network/structured-previews above, not a sidebar view you switch into
  // (it's a modal reachable from the Tools menu / command palette / "Compare selection…"), but it still
  // earns its own doc page + registry entry per [[maintain-in-app-docs-library]] — CPE-779 shipped the
  // dialog without one; this ticket closes that pre-existing gap since it needs somewhere to document the
  // new image sub-views anyway.
  compare: "32-compare",
  // CPE-1532 (epic CPE-1489, parent CPE-1530's store): the Drop Stack panel — the bottom-left toggle
  // handle + docked list of shelved items with per-item remove and clear-all. Like split-join/compare
  // above, not a sidebar view you switch into (it's a small dockable panel toggled from its own handle),
  // but it still earns its own doc page + registry entry per [[maintain-in-app-docs-library]].
  "drop-stack": "34-drop-stack",
};

/** The default doc when a section has no page (or an unknown id is passed): the Overview. */
export const DEFAULT_DOC_SLUG = "01-overview";

/** All section ids (exhaustive) — used by the guard test and any "which sections exist" logic. */
export const SECTIONS = Object.keys(SECTION_DOC) as Section[];

/**
 * The doc slug for a section. Falls back to the default when the id isn't mapped (graceful in prod);
 * the guard test ensures a mapped slug always resolves in `DOCS`, so this never lands on a blank page.
 */
export function docSlugForSection(section: Section | null | undefined): string {
  return (section && SECTION_DOC[section]) || DEFAULT_DOC_SLUG;
}

/** Every mapped slug (+ the default), for validating against `DOCS` in the guard test. */
export function mappedDocSlugs(): string[] {
  return [...Object.values(SECTION_DOC), DEFAULT_DOC_SLUG];
}

/** True iff a slug exists in the bundled library. */
export function docSlugExists(slug: string): boolean {
  return DOCS.some((d) => d.slug === slug);
}
