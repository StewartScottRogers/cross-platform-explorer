// CPE-1571 (epic CPE-1569, docs completeness): shipped-feature-without-a-doc guard. A dialog can ship
// (like BatchRenameDialog did, CPE-1571's own audit) without ever getting a line in the Documents
// library — this test makes that a CI failure going forward instead of something only caught by a
// human audit.
//
// Convention: every user-facing dialog is a `*Dialog.svelte` file in `src/lib/components` (mirrors the
// naming discipline the busy-cursor guard, `src/lib/invoke.guard.test.ts`, relies on). A dialog is
// "documented" if its feature name is mentioned, word-for-word, somewhere in the bundled `src/docs/*.md`
// library — a mechanical keyword scan, not a judgment about how *good* the coverage is.
//
// `OTHER_USER_FACING_SURFACES` is the escape hatch for a user-facing surface that ships without a
// `...Dialog.svelte` file (a panel, a popover, a sidebar section) — add its display name here and it's
// checked the same way. Empty today; nothing currently needs it (panels like Drop Stack / Terminal
// already have dedicated `sectionDocs.ts` entries + pages, guarded separately by `sectionDocs.test.ts`).
//
// `KNOWN_GAPS_ALLOWLIST` is the seed of CURRENT gaps this ticket found (CPE-1569 will close them one
// content slice at a time — shrink this list as each lands, never add to it for a NEW dialog). A brand
// new `*Dialog.svelte` with no doc mention is NOT allowlisted automatically, so it fails CI immediately —
// that's the whole point.
import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const COMPONENTS_DIR = join(process.cwd(), "src/lib/components");
const DOCS_DIR = join(process.cwd(), "src/docs");

/** Non-`*Dialog.svelte` user-facing surfaces to check the same way. Add a display name here if a new
 *  panel/popover/section ships without a dialog component and needs the same doc-mention guard. */
export const OTHER_USER_FACING_SURFACES: string[] = [];

/**
 * Known CURRENT gaps (docs-completeness audit, 2026-08-10 / CPE-1571). Each either has no doc page at
 * all yet, or is only named in passing (e.g. one bullet in the Explorer page's operations list) rather
 * than getting real coverage. CPE-1569's content slices close these one at a time — remove an entry here
 * the same PR that gives it real documentation. Never add an entry for a dialog that ships NEW without a
 * doc — that's the case this test exists to catch.
 */
export const KNOWN_GAPS_ALLOWLIST: string[] = [
  "ColorRulesDialog", // file coloring & labels rules editor — no page yet
  "ContentIndexSearchDialog", // local content-index semantic search — no page yet
  "FileNameSearchDialog", // recursive find-by-name — no page yet
  "InspectCryptoDialog", // dual-pane JWT/cert inspect overlay — no page yet
  "KeyboardBindingsDialog", // press-to-set rebind surface — no page yet (36-keyboard-shortcuts.md covers the read-only viewer only)
  "PasswordPromptDialog", // shared masked-input modal — no page yet
  // CPE-1575: real, current gap — organizing-select-by.md documents the feature in prose ("Select by
  // pattern…"), but the guard's PascalCase word-order check derives "pattern[\s-]*select" from this
  // class name, and no natural phrasing of "select by pattern" puts those words in that order. Not a
  // content gap; kept allowlisted purely because the mechanical name-order check can't see it.
  "PatternSelectDialog",
  "RepairLinkDialog", // broken-symlink repair flow — no page yet
  "SessionHistoryDialog", // Agent Watch session-history browser + export — no page yet
  "ShredConfirmDialog", // secure-delete (shred) confirmation — no page yet
  "SignCertDialog", // CA-signed certificate issuance — 28-cert-management.md covers create, not sign, by name
  "TransferConflictDialog", // copy/move conflict resolution (Replace/Skip/Keep both) — no page yet
  "VaultCreateDialog", // vault creation form — 20-vaults.md covers the feature but not this component name
  "WatchRulesDialog", // watched-folder trigger→action rules editor — no page yet
  "WorkspacesDialog", // saved window/tab-layout switcher — no page yet
];

/** Base name of every `*Dialog.svelte` file (no extension), e.g. "BatchRenameDialog". */
function dialogComponentNames(): string[] {
  return readdirSync(COMPONENTS_DIR)
    .filter((f) => f.endsWith("Dialog.svelte"))
    .map((f) => f.replace(/\.svelte$/, ""));
}

/** Every `src/docs/*.md` page's content, concatenated and lowercased, once. */
function docCorpus(): string {
  return readdirSync(DOCS_DIR)
    .filter((f) => f.endsWith(".md"))
    .map((f) => readFileSync(join(DOCS_DIR, f), "utf8"))
    .join("\n\n")
    .toLowerCase();
}

/**
 * Build a case-insensitive regex for a PascalCase surface name (optionally ending "Dialog") that matches
 * its words in order, separated by whitespace/hyphens (or nothing) — so "BatchRenameDialog" matches
 * "batch rename" or "Batch-Rename", and "Select By" matches "**Select by…**". Exported so the "would this
 * actually catch a gap" self-check below can exercise it directly.
 */
export function mentionRegex(surfaceName: string): RegExp {
  const base = surfaceName.replace(/Dialog$/, "");
  const words = base
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .split(/\s+/)
    .filter(Boolean)
    .map((w) => w.toLowerCase().replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  return new RegExp(words.join("[\\s-]*"), "i");
}

describe("docs coverage guard — shipped dialog without a doc mention (CPE-1571)", () => {
  it("every KNOWN_GAPS_ALLOWLIST entry is a real, currently-undocumented surface (allowlist doesn't rot)", () => {
    const dialogs = new Set(dialogComponentNames());
    const surfaces = new Set(OTHER_USER_FACING_SURFACES);
    const stale = KNOWN_GAPS_ALLOWLIST.filter((name) => !dialogs.has(name) && !surfaces.has(name));
    expect(
      stale,
      `these allowlist entries no longer match a shipped *Dialog.svelte or OTHER_USER_FACING_SURFACES ` +
        `name — rename/remove was reflected in code but not here: ${stale.join(", ")}`,
    ).toEqual([]);
  });

  it("every shipped dialog (outside the allowlist) is mentioned by name in some doc page", () => {
    const corpus = docCorpus();
    const allow = new Set(KNOWN_GAPS_ALLOWLIST);
    const surfaces = [...dialogComponentNames(), ...OTHER_USER_FACING_SURFACES];
    const undocumented = surfaces.filter((name) => !allow.has(name) && !mentionRegex(name).test(corpus));
    expect(
      undocumented,
      `these ship with no doc mention and aren't on KNOWN_GAPS_ALLOWLIST — add a doc page/section for the ` +
        `feature (preferred) or, if it's a genuinely pre-existing gap CPE-1569 hasn't closed yet, add it to ` +
        `KNOWN_GAPS_ALLOWLIST with a reason: ${undocumented.join(", ")}`,
    ).toEqual([]);
  });

  it("the mention check is not vacuous — it actually distinguishes present from absent", () => {
    // A feature the real docs genuinely cover (positive control).
    expect(mentionRegex("CompareDialog").test(docCorpus())).toBe(true);
    // A name with no plausible mention anywhere in the library (negative control) — proves a brand-new,
    // non-allowlisted, undocumented dialog would fail the assertion above rather than pass by accident.
    expect(mentionRegex("ZzyzxQuux9000Dialog").test(docCorpus())).toBe(false);
  });
});
