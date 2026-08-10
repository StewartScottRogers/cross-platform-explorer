import { describe, it, expect } from "vitest";
import {
  addRecent, removeRecent, togglePin, toggleFavorite, mergeLegacy,
  loadAutoRestore, saveAutoRestore, loadLastSession, saveLastSession,
  loadMetaColumnsForFolder, saveMetaColumnsForFolder,
  addNetworkLocation, removeNetworkLocation,
  loadNativeBridgeEnabled, saveNativeBridgeEnabled,
  loadSpotlightHotkeyEnabled, saveSpotlightHotkeyEnabled,
  loadSpotlightHotkeyChord, saveSpotlightHotkeyChord,
  DEFAULT_SPOTLIGHT_HOTKEY_CHORD,
  loadContentEmbedderEnabled, saveContentEmbedderEnabled,
  loadContentEmbedderBaseUrl, saveContentEmbedderBaseUrl,
  loadContentEmbedderModel, saveContentEmbedderModel,
  loadContentEmbedderConfig,
  loadDensity, saveDensity,
  loadTheme, saveTheme,
} from "./settings";
import type { RecentFile, Favorite } from "./types";
import type { WorkspaceTab } from "./workspaces";
import type { ActiveMetaColumn } from "./columns";

describe("auto-restore session settings (CPE-789)", () => {
  it("defaults to off with no saved session", () => {
    expect(loadAutoRestore()).toBe(false);
    expect(loadLastSession()).toEqual([]);
  });

  it("round-trips the flag and the captured tabs", () => {
    saveAutoRestore(true);
    expect(loadAutoRestore()).toBe(true);
    const tabs: WorkspaceTab[] = [{ path: "/a", view: "list" }, { path: "/b" }];
    saveLastSession(tabs);
    expect(loadLastSession()).toEqual(tabs);
    saveAutoRestore(false); // reset module state for other tests
  });

  it("drops corrupt tabs from a persisted session (tolerant parse)", () => {
    saveLastSession([{ path: "/ok" }, { path: "" }, { bogus: 1 } as unknown as WorkspaceTab]);
    expect(loadLastSession()).toEqual([{ path: "/ok" }]);
    saveLastSession([]); // reset
  });
});

// CPE-1146 (epic CPE-707): the active metadata-column set persists PER FOLDER, keyed by absolute path,
// with "none saved" degrading to an empty array (the default: a fresh folder shows only the built-ins).
describe("metaColumnsByFolder (CPE-1146)", () => {
  it("defaults to empty for a folder with nothing saved", () => {
    expect(loadMetaColumnsForFolder("/never/saved")).toEqual([]);
  });

  it("round-trips a saved column set for its own folder, leaving other folders untouched", () => {
    const cols: ActiveMetaColumn[] = [{ id: "dimensions", width: 120 }, { id: "duration", width: 90 }];
    saveMetaColumnsForFolder("/photos", cols);
    expect(loadMetaColumnsForFolder("/photos")).toEqual(cols);
    expect(loadMetaColumnsForFolder("/music")).toEqual([]); // a different folder is unaffected
    saveMetaColumnsForFolder("/photos", []); // reset for other tests
  });

  it("saving an empty set for a folder that HAD one restores the default-empty (folder pruned, not stored as [])", () => {
    saveMetaColumnsForFolder("/temp", [{ id: "dimensions", width: 100 }]);
    expect(loadMetaColumnsForFolder("/temp")).toEqual([{ id: "dimensions", width: 100 }]);
    saveMetaColumnsForFolder("/temp", []);
    expect(loadMetaColumnsForFolder("/temp")).toEqual([]);
  });

  it("re-clamps a corrupt/too-narrow persisted width on load (CPE-1140-style guard)", () => {
    saveMetaColumnsForFolder("/clamp-me", [{ id: "dimensions", width: 3 }]);
    expect(loadMetaColumnsForFolder("/clamp-me")[0].width).toBeGreaterThanOrEqual(70); // META_COL_MIN
    saveMetaColumnsForFolder("/clamp-me", []); // reset
  });

  it("degrades a corrupt entry (wrong shape) to empty rather than crashing", () => {
    saveMetaColumnsForFolder("/bad", [{ bogus: 1 } as unknown as ActiveMetaColumn]);
    expect(loadMetaColumnsForFolder("/bad")).toEqual([]);
  });
});

// CPE-1177 (epic CPE-717): the native-bridge opt-in that gates TagEditor's native pull/push controls
// (CPE-1177) and PropertiesDialog's read-only Native metadata section (CPE-1176). OFF by default so
// the plain explorer never touches OS-native file metadata unless the user turns it on.
describe("nativeBridgeEnabled (CPE-1177)", () => {
  it("defaults to off", () => {
    expect(loadNativeBridgeEnabled()).toBe(false);
  });

  it("round-trips through persist", () => {
    saveNativeBridgeEnabled(true);
    expect(loadNativeBridgeEnabled()).toBe(true);
    saveNativeBridgeEnabled(false);
    expect(loadNativeBridgeEnabled()).toBe(false);
  });
});

// CPE-1215 (epic CPE-704): the global-hotkey opt-in that claims/releases an OS-wide chord (via
// tauri-plugin-global-shortcut) which fires `spotlight:open`. OFF by default — no background OS
// registration cost unless the user turns it on in Settings. The chord persists independently so a
// custom shortcut survives toggling the feature off and back on.
describe("spotlightHotkeyEnabled / spotlightHotkeyChord (CPE-1215)", () => {
  it("defaults to off with the default chord", () => {
    expect(loadSpotlightHotkeyEnabled()).toBe(false);
    expect(loadSpotlightHotkeyChord()).toBe(DEFAULT_SPOTLIGHT_HOTKEY_CHORD);
  });

  it("round-trips the enabled flag", () => {
    saveSpotlightHotkeyEnabled(true);
    expect(loadSpotlightHotkeyEnabled()).toBe(true);
    saveSpotlightHotkeyEnabled(false);
    expect(loadSpotlightHotkeyEnabled()).toBe(false);
  });

  it("round-trips a custom chord independently of the enabled flag", () => {
    saveSpotlightHotkeyChord("Alt+Space");
    expect(loadSpotlightHotkeyChord()).toBe("Alt+Space");
    saveSpotlightHotkeyChord(DEFAULT_SPOTLIGHT_HOTKEY_CHORD); // reset for other tests
  });
});

// CPE-1273 (epic CPE-976): the configurable real embedder for content search. OFF by default so content
// search keeps using the local dependency-free embedder (no key, no network). The enabled/URL/model
// persist here; the API KEY never does (it lives only in the OS keychain, via content_embedder_set_key).
describe("contentEmbedder config (CPE-1273)", () => {
  it("defaults to disabled with blank endpoint + model", () => {
    expect(loadContentEmbedderEnabled()).toBe(false);
    expect(loadContentEmbedderBaseUrl()).toBe("");
    expect(loadContentEmbedderModel()).toBe("");
    expect(loadContentEmbedderConfig()).toEqual({ enabled: false, base_url: "", model: "" });
  });

  it("round-trips the enabled flag, endpoint, and model", () => {
    saveContentEmbedderEnabled(true);
    saveContentEmbedderBaseUrl("http://localhost:1234/v1");
    saveContentEmbedderModel("text-embedding-3-small");
    expect(loadContentEmbedderConfig()).toEqual({
      enabled: true,
      base_url: "http://localhost:1234/v1",
      model: "text-embedding-3-small",
    });
    // reset for other tests
    saveContentEmbedderEnabled(false);
    saveContentEmbedderBaseUrl("");
    saveContentEmbedderModel("");
  });

  it("the assembled config carries NO api key field (the key lives only in the keychain)", () => {
    saveContentEmbedderEnabled(true);
    const cfg = loadContentEmbedderConfig();
    expect(Object.keys(cfg).sort()).toEqual(["base_url", "enabled", "model"]);
    expect(cfg).not.toHaveProperty("api_key");
    expect(cfg).not.toHaveProperty("key");
    saveContentEmbedderEnabled(false);
  });
});

// CPE-1526 (foundation slice of epic CPE-1488 "compact/dense view mode"): the persisted density
// setting. "comfortable" is today's only behavior and stays the default so this ticket has zero
// visible effect; the delete-test requires an absent/corrupt stored value to degrade to it cleanly
// rather than crash (mirrors the isView-style validators already in this file).
describe("density (CPE-1526)", () => {
  it("defaults to comfortable with nothing saved", () => {
    expect(loadDensity()).toBe("comfortable");
  });

  it("round-trips compact through save/load", () => {
    saveDensity("compact");
    expect(loadDensity()).toBe("compact");
    saveDensity("comfortable"); // reset for other tests
  });

  it("degrades a corrupt/invalid stored value to the default rather than crashing", () => {
    saveDensity("ultra-cozy" as unknown as ReturnType<typeof loadDensity>);
    expect(loadDensity()).toBe("comfortable");
  });
});

// CPE-1535/CPE-1540 (epic CPE-1492 "light/dark theme"): the persisted theme setting. "system" is the
// default and resolves live against the OS prefers-color-scheme signal (see theme.ts's resolveTheme);
// "light" and "dark" are explicit overrides. The delete-test requires an absent/corrupt stored value to
// degrade to "system" cleanly rather than crash (mirrors the density validator immediately above).
describe("theme (CPE-1535/CPE-1540)", () => {
  it("defaults to system with nothing saved", () => {
    expect(loadTheme()).toBe("system");
  });

  it("round-trips light through save/load", () => {
    saveTheme("light");
    expect(loadTheme()).toBe("light");
    saveTheme("system"); // reset for other tests
  });

  it("round-trips dark through save/load", () => {
    saveTheme("dark");
    expect(loadTheme()).toBe("dark");
    saveTheme("system"); // reset for other tests
  });

  it("degrades a corrupt/invalid stored value to the default rather than crashing", () => {
    saveTheme("solarized" as unknown as ReturnType<typeof loadTheme>);
    expect(loadTheme()).toBe("system");
  });
});

describe("mergeLegacy (localStorage → settings.json migration, CPE-226)", () => {
  it("backfills keys the file lacks from legacy localStorage values", () => {
    const ls: Record<string, string> = {
      "cpe.view": JSON.stringify("list"),
      "cpe.sidebarWidth": JSON.stringify(260),
    };
    const merged = mergeLegacy({}, (k) => ls[k] ?? null);
    expect(merged["cpe.view"]).toBe("list");
    expect(merged["cpe.sidebarWidth"]).toBe(260);
  });

  it("lets the file win over localStorage for keys it already has", () => {
    const ls: Record<string, string> = { "cpe.view": JSON.stringify("icons") };
    const merged = mergeLegacy({ "cpe.view": "details" }, (k) => ls[k] ?? null);
    expect(merged["cpe.view"]).toBe("details");
  });

  it("ignores an unparseable legacy value", () => {
    const merged = mergeLegacy({}, (k) => (k === "cpe.view" ? "not json" : null));
    expect("cpe.view" in merged).toBe(false);
  });
});

const r = (path: string, opened: number): RecentFile => ({
  path,
  name: path.split("/").pop() ?? path,
  opened,
});

describe("addRecent", () => {
  it("puts the newest entry first", () => {
    const list = addRecent([], { path: "/a.txt", name: "a.txt" }, 100);
    expect(list[0].path).toBe("/a.txt");
    expect(list[0].opened).toBe(100);
  });

  it("de-duplicates by path and moves the entry to the front", () => {
    let list = [r("/a.txt", 1), r("/b.txt", 2)];
    list = addRecent(list, { path: "/b.txt", name: "b.txt" }, 300);
    expect(list.map((x) => x.path)).toEqual(["/b.txt", "/a.txt"]);
    expect(list).toHaveLength(2);
    expect(list[0].opened).toBe(300);
  });

  it("caps the list so it cannot grow without bound", () => {
    let list: RecentFile[] = [];
    for (let i = 0; i < 40; i++) {
      list = addRecent(list, { path: `/f${i}.txt`, name: `f${i}.txt` }, i);
    }
    expect(list).toHaveLength(20);
    expect(list[0].path).toBe("/f39.txt"); // newest retained
    expect(list.some((x) => x.path === "/f0.txt")).toBe(false); // oldest evicted
  });
});

describe("removeRecent (CPE-341)", () => {
  it("drops only the matching path and keeps the rest in order", () => {
    const list = [r("/a.txt", 3), r("/b.txt", 2), r("/c.txt", 1)];
    expect(removeRecent(list, "/b.txt").map((x) => x.path)).toEqual(["/a.txt", "/c.txt"]);
  });

  it("is a no-op when the path is absent, and does not mutate the input", () => {
    const list = [r("/a.txt", 1)];
    expect(removeRecent(list, "/z.txt")).toEqual(list);
    expect(list.map((x) => x.path)).toEqual(["/a.txt"]);
  });
});

describe("togglePin", () => {
  it("adds a pin when absent", () => {
    expect(togglePin([], "/a")).toEqual(["/a"]);
  });

  it("removes a pin when present", () => {
    expect(togglePin(["/a", "/b"], "/a")).toEqual(["/b"]);
  });

  it("does not mutate the input", () => {
    const pins = ["/a"];
    togglePin(pins, "/b");
    expect(pins).toEqual(["/a"]);
  });
});

describe("network locations (CPE-1163)", () => {
  it("adds a trimmed location and de-duplicates case/slash-insensitively", () => {
    let list = addNetworkLocation([], "  \\\\server\\share  ");
    expect(list).toEqual(["\\\\server\\share"]);
    // A trailing-slash / different-case variant is treated as the same entry (re-added at the end).
    list = addNetworkLocation(list, "\\\\SERVER\\share\\");
    expect(list).toEqual(["\\\\SERVER\\share\\"]);
    // A genuinely different location is appended.
    list = addNetworkLocation(list, "smb://nas/media");
    expect(list).toEqual(["\\\\SERVER\\share\\", "smb://nas/media"]);
  });

  it("ignores an empty/whitespace address", () => {
    expect(addNetworkLocation(["\\\\a\\b"], "   ")).toEqual(["\\\\a\\b"]);
  });

  it("removes a location by path (slash/case-insensitive)", () => {
    expect(removeNetworkLocation(["\\\\server\\share", "smb://nas/media"], "\\\\SERVER\\SHARE\\"))
      .toEqual(["smb://nas/media"]);
  });
});

describe("toggleFavorite (CPE-338)", () => {
  const file = { path: "/a.txt", name: "a.txt", is_dir: false };
  const dir = { path: "/docs", name: "docs", is_dir: true };

  it("adds a favorite (file or folder) when absent, preserving is_dir", () => {
    const list = toggleFavorite([], dir);
    expect(list).toEqual([{ path: "/docs", name: "docs", is_dir: true }]);
    expect(toggleFavorite(list, file).map((f) => f.path)).toEqual(["/docs", "/a.txt"]);
  });

  it("removes a favorite when the path is already present", () => {
    const list: Favorite[] = [
      { path: "/docs", name: "docs", is_dir: true },
      { path: "/a.txt", name: "a.txt", is_dir: false },
    ];
    expect(toggleFavorite(list, dir).map((f) => f.path)).toEqual(["/a.txt"]);
  });

  it("does not mutate the input", () => {
    const list: Favorite[] = [{ path: "/a.txt", name: "a.txt", is_dir: false }];
    toggleFavorite(list, dir);
    expect(list).toEqual([{ path: "/a.txt", name: "a.txt", is_dir: false }]);
  });
});
