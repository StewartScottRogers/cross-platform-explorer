// CPE-1676: layout guard for the Epics queue. `Ticketing/Epics/` has the SAME five status folders as
// `Ticketing/Tickets/`, and — as there — **the folder is the authoritative status**. Both board
// implementations and the ticket MCP read those folders directly, so a file put back at the old flat
// location, or a frontmatter `status:` that drifts from its folder, makes the board silently lie.
// These two tests are that invariant, in the same class as `sectionDocs.test.ts` for the docs registry.
import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const EPICS = join(process.cwd(), "Ticketing", "Epics");

/** The five status folders, and the epic `status:` each one means. Mirrors
 *  `cpe_server::ticket_board::epic_status_for_folder` (and the sidecar board's copy of it). */
export const EPIC_STATUS_FOLDERS: Record<string, string> = {
  Backlog: "Proposed",
  Doing: "In Progress",
  Blocked: "Blocked",
  Deferred: "Deferred",
  Done: "Done",
};

/** The `status:` value from a ticket's YAML frontmatter, or "" when there is none. */
function frontmatterStatus(md: string): string {
  const body = md.replace(/^﻿/, "");
  const fm = /^---\r?\n([\s\S]*?)\r?\n---/.exec(body);
  if (!fm) return "";
  const m = /^status:\s*(.*)$/m.exec(fm[1]);
  return m ? m[1].trim().replace(/^["']|["']$/g, "") : "";
}

/** The `tags: [a, b]` list from a ticket's frontmatter. A non-list value parses as `[]`, exactly as
 *  both boards' `parse_tags` does — which is the whole point: a bare `tags: reliability` is invisible
 *  to them. */
function frontmatterTags(md: string): string[] {
  const fm = /^---\r?\n([\s\S]*?)\r?\n---/.exec(md.replace(/^﻿/, ""));
  if (!fm) return [];
  const m = /^tags:\s*(.*)$/m.exec(fm[1]);
  const raw = m ? m[1].trim() : "";
  if (!raw.startsWith("[") || !raw.endsWith("]")) return [];
  return raw
    .slice(1, -1)
    .split(",")
    .map((t) => t.trim().replace(/^["']|["']$/g, ""))
    .filter(Boolean);
}

/** Every `CPE-*.md` in the queue, as `{ folder, name, status, tags }`. */
function epicFiles(): { folder: string; name: string; status: string; tags: string[] }[] {
  const out: { folder: string; name: string; status: string; tags: string[] }[] = [];
  for (const folder of Object.keys(EPIC_STATUS_FOLDERS)) {
    for (const name of readdirSync(join(EPICS, folder))) {
      if (!name.startsWith("CPE-") || !name.endsWith(".md")) continue;
      const md = readFileSync(join(EPICS, folder, name), "utf8");
      out.push({ folder, name, status: frontmatterStatus(md), tags: frontmatterTags(md) });
    }
  }
  return out;
}

describe("Epics queue layout (CPE-1676)", () => {
  it("Ticketing/Epics/ holds only the five status folders — no epic left at the old flat location", () => {
    const entries = readdirSync(EPICS);
    const dirs = entries.filter((n) => statSync(join(EPICS, n)).isDirectory());
    const loose = entries.filter((n) => !statSync(join(EPICS, n)).isDirectory());

    expect(
      loose,
      `Ticketing/Epics/ must contain only the five status folders. Loose file(s) found: ${loose.join(", ")}. ` +
        "Move each epic into the folder matching its status (Proposed → Backlog/, In Progress → Doing/, Done → Done/).",
    ).toEqual([]);
    expect([...dirs].sort()).toEqual([...Object.keys(EPIC_STATUS_FOLDERS)].sort());
    // The migration is not vacuously satisfiable: there really are epics in there.
    expect(epicFiles().length).toBeGreaterThan(0);
  });

  it("every epic's frontmatter `status:` agrees with the folder it sits in", () => {
    const mismatched = epicFiles()
      .filter((f) => f.status !== EPIC_STATUS_FOLDERS[f.folder])
      .map((f) => `${f.folder}/${f.name} has status "${f.status}", expected "${EPIC_STATUS_FOLDERS[f.folder]}"`);
    expect(
      mismatched,
      "Folder location is the authoritative status — move the file or fix its `status:` line so they agree:\n" +
        mismatched.join("\n"),
    ).toEqual([]);
  });

  // Found by actually running both boards during CPE-1676: CPE-862 carried `tags: reliability`
  // (a bare scalar, not a list), so `parse_tags` returned [] and neither board had ever listed it.
  // The queue looked fine on disk while the boards silently omitted an epic — precisely the failure
  // mode folder-authority is meant to prevent, so it gets a guard of its own.
  it("every epic is `epic`-tagged, so both boards actually list it", () => {
    const untagged = epicFiles()
      .filter((f) => !f.tags.includes("epic"))
      .map((f) => `${f.folder}/${f.name} has tags [${f.tags.join(", ")}]`);
    expect(
      untagged,
      "Both boards filter the queue by the `epic` tag, so an epic without it is invisible rather than " +
        "wrong. `tags:` must be a LIST containing `epic` (a bare `tags: reliability` parses as empty):\n" +
        untagged.join("\n"),
    ).toEqual([]);
  });
});
