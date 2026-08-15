// CPE-1737 round 3, guard rewritten by the Foreman after the round-3 review.
//
// This suite exists because the guard it replaces could not fail. `settings.test.ts`'s round-3 test
// round-trips `saveMetaColumnsForFolder` → `loadMetaColumnsForFolder` **within one version**, so both
// halves agree by construction: restore round 2 in full (canonicalise the key on BOTH save and load) and
// that test still passes, 63/63, with CI green — while every column set a prior release wrote under a
// Windows backslash key is unreachable. A guard that survives full restoration of the bug is not a guard.
//
// The real defect only exists ACROSS versions, so the test has to be too: seed a settings document
// exactly as a release before CPE-1737 wrote it (raw `currentPath` keys, backslashes and all), hydrate
// through the real `initSettings`, and read. Round 2's load looked up `canonicalPath(path)` — which
// rewrites `\` to `/` — against keys that were never canonical, so it missed every one of them and
// returned [].
import { describe, it, expect, vi, beforeEach } from "vitest";

// vi.mock is hoisted. `initSettings` reads through `commands.readSettings`, and `schedulePersist` writes
// back through `commands.writeSettings`; both are mocked so this suite touches no backend and no disk.
const { readSettings, writeSettings } = vi.hoisted(() => ({
  readSettings: vi.fn(),
  writeSettings: vi.fn(),
}));
vi.mock("./bindings.gen", () => ({ commands: { readSettings, writeSettings } }));

import { initSettings, loadMetaColumnsForFolder, saveMetaColumnsForFolder } from "./settings";
import type { ActiveMetaColumn } from "./columns";

/** A settings.json exactly as every release BEFORE CPE-1737 wrote it: `metaColumnsByFolder` keyed on the
 *  raw `currentPath`, with no rewrite of any kind applied — so a local Windows folder appears under its
 *  real backslash path. */
const PRIOR_RELEASE_DOCUMENT = {
  // The on-disk key is namespaced (`KEYS.metaColumnsByFolder`), which is what a real settings.json holds.
  "cpe.metaColumnsByFolder": {
    "C:\\Users\\me\\docs": [{ id: "dimensions", width: 110 }],
    "sftp://h/srv/sub": [{ id: "pages", width: 90 }],
    "/home/me/photos": [{ id: "duration", width: 80 }],
  },
};

const hydrateFromPriorRelease = async (): Promise<void> => {
  // `initSettings` reads through `unwrap`, so the mock must return the contract envelope the typed
  // client produces, not the bare string.
  readSettings.mockResolvedValue({ status: "ok", data: JSON.stringify(PRIOR_RELEASE_DOCUMENT) });
  writeSettings.mockResolvedValue({ status: "ok", data: null });
  await initSettings();
};

describe("metaColumnsByFolder survives an upgrade from a pre-CPE-1737 release", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("finds a Windows backslash key written by a prior release", async () => {
    // THE regression. Round 2 canonicalised on both sides, so this key — the shape every Windows user
    // actually has on disk — became unreachable and the user's column widths silently reset.
    await hydrateFromPriorRelease();
    expect(loadMetaColumnsForFolder("C:\\Users\\me\\docs")).toEqual([{ id: "dimensions", width: 110 }]);
  });

  it("finds a POSIX key written by a prior release", async () => {
    await hydrateFromPriorRelease();
    expect(loadMetaColumnsForFolder("/home/me/photos")).toEqual([{ id: "duration", width: 80 }]);
  });

  it("still resolves a remote directory reached by its now-slashed listing-row path", async () => {
    // The canonical fallback is what CPE-1737 round 1 needs: the row click yields `…/sub/` while the
    // stored key (and every other route to that folder) is `…/sub`.
    await hydrateFromPriorRelease();
    expect(loadMetaColumnsForFolder("sftp://h/srv/sub/")).toEqual([{ id: "pages", width: 90 }]);
  });

  it("re-saving a prior-release Windows folder keeps its key raw, so the next upgrade finds it too", async () => {
    await hydrateFromPriorRelease();
    const cols: ActiveMetaColumn[] = [{ id: "dimensions", width: 140 }];
    saveMetaColumnsForFolder("C:\\Users\\me\\docs", cols);

    // The persist is debounced (150ms); wait for it rather than reaching into module internals.
    await vi.waitFor(() => expect(writeSettings).toHaveBeenCalled(), { timeout: 3000 });

    // Read the document the way a FUTURE release would — straight out of what was persisted — rather
    // than through this version's loader, which would resolve the key either way and hide a rewrite.
    const persisted = JSON.parse(
      (writeSettings.mock.calls.at(-1) as [string])[0],
    ) as Record<string, Record<string, unknown>>;
    const keys = Object.keys(persisted["cpe.metaColumnsByFolder"]);
    expect(keys).toContain("C:\\Users\\me\\docs");
    expect(keys.some((k) => k.includes("C:/"))).toBe(false);
    expect(persisted["cpe.metaColumnsByFolder"]["C:\\Users\\me\\docs"]).toEqual(cols);
  });
});
