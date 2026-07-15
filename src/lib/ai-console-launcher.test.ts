/**
 * AI Console launcher UI harness (CPE-388).
 *
 * The launcher is an inline HTML/JS page embedded in the ai-console sidecar and rendered in a
 * WebView2 pane we can't drive headlessly — so its rendering + wiring had no automated coverage,
 * which is where the recent user-facing bugs came from. This loads the *real* launcher script into
 * jsdom with stubbed xterm/WebSocket and a mock `fetch`, so we can unit-test behaviour headlessly.
 */
import { describe, it, expect, vi } from "vitest";
import { JSDOM } from "jsdom";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const HTML = readFileSync(resolve(process.cwd(), "sidecar/ai-console/src/launcher.html"), "utf8");
// The DOM (everything before the first <script>) and the inline app script (the last <script>).
const BODY = HTML.match(/<body[^>]*>([\s\S]*?)<\/body>/)![1].split(/<script/)[0];
const APP = [...HTML.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g)].map((m) => m[1]).at(-1)!;

/** A default catalog so the launcher's boot `load()` succeeds; override per test via routes. */
function defaultCatalog() {
  return {
    agents: [
      { id: "claude", name: "Claude Code", installed: true, providers: ["openrouter", "native"], defaultModel: "claude-sonnet-4-5" },
    ],
    cwd: "/repo",
    presets: { lastAgent: "claude", agents: { claude: { presets: [], lastUsed: null } }, credentials: [], onboarded: true, autoUpdateCatalog: false, pinnedAgents: [] },
  };
}

/** Mount the launcher in a fresh jsdom. `routes(path, opts) => data | {ok,status,data}`. */
async function mountLauncher(routes: (path: string, opts: any) => any = () => ({})) {
  const dom = new JSDOM(`<!doctype html><html><head></head><body>${BODY}</body></html>`, {
    runScripts: "dangerously",
    pretendToBeVisual: true,
    url: "http://127.0.0.1/",
  });
  const w = dom.window as any;
  class FakeTerm {
    unicode = { activeVersion: "" };
    rows = 24;
    cols = 80;
    loadAddon() {}
    open() {}
    write(_d: unknown, cb?: () => void) { cb && cb(); }
    reset() {}
    onData() {} onScroll() {} attachCustomKeyEventHandler() {} focus() {}
    scrollToBottom() {} scrollToTop() {} scrollToLine() {} dispose() {}
    get buffer() { return { active: { baseY: 0, viewportY: 0 } }; }
  }
  w.Terminal = FakeTerm;
  w.FitAddon = { FitAddon: class { fit() {} } };
  w.SearchAddon = { SearchAddon: class {} };
  w.WebLinksAddon = { WebLinksAddon: class {} };
  w.Unicode11Addon = { Unicode11Addon: class {} };
  w.WebSocket = class { close() {} send() {} };
  w.ResizeObserver = class { observe() {} disconnect() {} };
  w.requestAnimationFrame = (cb: (t: number) => void) => cb(0);

  const fetchMock = vi.fn(async (path: string, opts: any) => {
    const r = routes(path, opts);
    const has = r && typeof r === "object" && ("ok" in r || "status" in r || "data" in r);
    const ok = has ? r.ok ?? true : true;
    const status = has ? r.status ?? (ok ? 200 : 400) : 200;
    // /api/catalog defaults to the base catalog so boot never fails.
    const data = has ? r.data ?? {} : r ?? {};
    const body = path === "/api/catalog" && (!data || Object.keys(data).length === 0) ? defaultCatalog() : data;
    return { ok, status, text: async () => JSON.stringify(body) };
  });
  w.fetch = fetchMock;

  const s = w.document.createElement("script");
  s.textContent = APP;
  w.document.body.appendChild(s);
  await new Promise((r) => setTimeout(r, 0)); // let boot load() settle
  return { w, fetchMock };
}

describe("AI Console launcher — Keys panel error hints (CPE-386)", () => {
  it("turns a 'Secrets not granted' error into an actionable message; passes others through", async () => {
    const { w } = await mountLauncher();
    expect(w.permHint("Secrets is not granted to 'ai-console'")).toMatch(/reopen the AI Console/i);
    expect(w.permHint("secrets denied")).toMatch(/Allow for Secrets/i);
    expect(w.permHint("bad json: x")).toBe("bad json: x");
  });
});

describe("AI Console launcher — named sets / presets (the reported confusion)", () => {
  it("renders the current agent's sets into the dropdown, with a blank placeholder", async () => {
    const { w } = await mountLauncher();
    w.eval(`catalog = ${JSON.stringify({ ...defaultCatalog(), presets: { agents: { claude: { presets: [{ name: "Work", provider: "openrouter", model: "sonnet" }] } } } })}`);
    w.document.getElementById("agent").value = "claude";
    w.renderPresets();
    const opts = [...w.document.getElementById("preset").options].map((o: any) => o.value);
    expect(opts).toEqual(["", "Work"]);
  });

  it("saveSet with an empty name warns and does NOT hit the API", async () => {
    const { w, fetchMock } = await mountLauncher();
    fetchMock.mockClear();
    w.document.getElementById("set-name").value = "   ";
    await w.saveSet();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(w.document.getElementById("msg").textContent).toMatch(/type a name/i);
  });

  it("saveSet with a name POSTs the agent config (not the key) to /api/presets", async () => {
    const posts: any[] = [];
    const { w } = await mountLauncher((path, opts) => {
      if (path === "/api/presets") posts.push(JSON.parse(opts.body));
      return {};
    });
    w.document.getElementById("agent").value = "claude";
    w.document.getElementById("provider").value = "openrouter";
    w.document.getElementById("model").value = "sonnet";
    w.document.getElementById("set-name").value = "Work";
    await w.saveSet();
    expect(posts).toHaveLength(1);
    expect(posts[0]).toMatchObject({ agent: "claude", name: "Work", provider: "openrouter", model: "sonnet" });
    expect(posts[0]).not.toHaveProperty("key"); // it saves the config, never the key
  });
});

describe("AI Console launcher — catalog controls", () => {
  it("renderCatalogControls reflects the persisted auto-update + pin state", async () => {
    const { w } = await mountLauncher();
    w.eval(`catalog = ${JSON.stringify({ ...defaultCatalog(), presets: { agents: { claude: {} }, autoUpdateCatalog: true, pinnedAgents: ["claude"] } })}`);
    w.document.getElementById("agent").value = "claude";
    w.renderCatalogControls();
    expect(w.document.getElementById("auto-update").checked).toBe(true);
    expect(w.document.getElementById("pin-agent").checked).toBe(true);
  });

  it("version rollback (CPE-383): picker lists published versions and rolls the selected agent back", async () => {
    const posts: any[] = [];
    const { w } = await mountLauncher((path, opts) => {
      if (path === "/api/catalog/versions")
        return {
          versions: [
            { tag: "v0.1.0", publishedAt: "2026-07-01T00:00:00Z", prerelease: false },
            { tag: "v0.0.9", publishedAt: "2026-06-01T00:00:00Z", prerelease: true },
          ],
        };
      if (path === "/api/catalog/rollback") {
        posts.push(JSON.parse(opts.body));
        return { indexOk: true, applied: 1, tag: "v0.1.0", agents: 1 };
      }
      return {};
    });
    w.document.getElementById("agent").value = "claude";
    // Open the picker from the Manage menu → overlay shows, populated with the versions.
    w.document.getElementById("rollback-agents").click();
    await new Promise((r) => setTimeout(r, 0));
    expect(w.document.getElementById("rollback-overlay").hidden).toBe(false);
    expect(w.document.getElementById("rollback-agent").textContent).toBe("Claude Code");
    const opts = [...w.document.getElementById("rollback-version").options].map((o: any) => o.value);
    expect(opts).toEqual(["v0.1.0", "v0.0.9"]);
    // Apply → POSTs {tag, agents:[currentAgent]} and closes the overlay.
    w.document.getElementById("rollback-version").value = "v0.1.0";
    w.document.getElementById("rollback-apply").click();
    await new Promise((r) => setTimeout(r, 0));
    expect(posts).toEqual([{ tag: "v0.1.0", agents: ["claude"] }]);
    expect(w.document.getElementById("rollback-overlay").hidden).toBe(true);
    expect(w.document.getElementById("msg").textContent).toMatch(/rolled back to v0\.1\.0/i);
  });

  it("version rollback with no published versions shows a message and no overlay", async () => {
    const { w } = await mountLauncher((path) =>
      path === "/api/catalog/versions" ? { versions: [] } : {},
    );
    w.document.getElementById("rollback-agents").click();
    await new Promise((r) => setTimeout(r, 0));
    expect(w.document.getElementById("rollback-overlay").hidden).toBe(true);
    expect(w.document.getElementById("msg").textContent).toMatch(/no prior versions/i);
  });
});

describe("AI Console launcher — Help panel + Manage menu (CPE-390)", () => {
  it("the ? button opens the Help panel and × closes it", async () => {
    const { w } = await mountLauncher();
    const overlay = w.document.getElementById("help-overlay");
    expect(overlay.hidden).toBe(true);
    w.document.getElementById("help-btn").click();
    expect(overlay.hidden).toBe(false);
    expect(w.document.getElementById("help-body").textContent).toMatch(/does not save your key|Preset/i);
    w.document.getElementById("help-close").click();
    expect(overlay.hidden).toBe(true);
  });

  it("Manage agents ▾ toggles its menu, which holds the moved advanced controls; an action closes it", async () => {
    const { w } = await mountLauncher((path) =>
      path === "/api/catalog/refresh" ? { indexOk: true, applied: 0, agents: 1 } : {},
    );
    const menu = w.document.getElementById("manage-menu");
    expect(menu.hidden).toBe(true);
    w.document.getElementById("manage-btn").click();
    expect(menu.hidden).toBe(false);
    // the advanced controls now live INSIDE the menu (same ids → wiring unchanged)
    expect(menu.contains(w.document.getElementById("auto-update"))).toBe(true);
    expect(menu.contains(w.document.getElementById("pin-agent"))).toBe(true);
    expect(menu.contains(w.document.getElementById("refresh-agents"))).toBe(true);
    // a menu action (Check for updates) closes the menu
    w.document.getElementById("refresh-agents").click();
    await new Promise((r) => setTimeout(r, 0));
    expect(menu.hidden).toBe(true);
  });
});

describe("AI Console launcher — inexperienced-user goal (CPE-392/393/394)", () => {
  it("optional fields are collapsed under Advanced ▾ by default; the toggle reveals them", async () => {
    const { w } = await mountLauncher();
    const adv = w.document.getElementById("advanced-row");
    expect(adv.hidden).toBe(true);
    for (const id of ["smallModel", "apiKey", "preset", "set-save"]) {
      expect(adv.contains(w.document.getElementById(id))).toBe(true);
    }
    w.document.getElementById("advanced-btn").click();
    expect(adv.hidden).toBe(false);
    w.document.getElementById("advanced-btn").click();
    expect(adv.hidden).toBe(true);
  });

  it("providerNeedsKey distinguishes paid providers from built-in / local logins", async () => {
    const { w } = await mountLauncher();
    expect(w.providerNeedsKey("openrouter")).toBe(true);
    expect(w.providerNeedsKey("anthropic")).toBe(true);
    expect(w.providerNeedsKey("native")).toBe(false);
    expect(w.providerNeedsKey("lmstudio-local")).toBe(false);
  });

  it("defaults a keyless first-timer to a no-key provider, and warns only when a paid one is chosen", async () => {
    const { w } = await mountLauncher(); // default catalog: claude [openrouter, native], no keys
    expect(w.document.getElementById("provider").value).toBe("native");
    expect(w.document.getElementById("msg").textContent).not.toMatch(/needs an API key/i);
    // pick the paid provider → readiness hint appears
    w.document.getElementById("provider").value = "openrouter";
    w.checkLaunchReady();
    expect(w.document.getElementById("msg").textContent).toMatch(/needs an API key/i);
    // typing a key clears the hint
    w.document.getElementById("apiKey").value = "sk-or-abc";
    w.checkLaunchReady();
    expect(w.document.getElementById("msg").textContent).not.toMatch(/needs an API key/i);
  });

  it("the first-run guide's 'Add an API key' opens the Keys panel", async () => {
    const { w } = await mountLauncher();
    w.document.getElementById("onboard-overlay").hidden = false; // simulate first run
    w.document.getElementById("onboard-addkey").click();
    expect(w.document.getElementById("onboard-overlay").hidden).toBe(true);
    expect(w.document.getElementById("keys-overlay").hidden).toBe(false);
  });
});

describe("AI Console launcher — Browse ↔ Project folder sync (CPE-395)", () => {
  it("Browse opens the picker at the typed Project folder, and writes the choice back", async () => {
    const posts: any[] = [];
    const { w } = await mountLauncher((path, opts) => {
      if (path === "/api/pick-folder") {
        posts.push(JSON.parse(opts.body));
        return { path: "/picked/dir" };
      }
      return {};
    });
    w.document.getElementById("cwd").value = "/my/project";
    await w.pickFolder();
    expect(posts).toEqual([{ start: "/my/project" }]); // opens AT the current box value
    expect(w.document.getElementById("cwd").value).toBe("/picked/dir"); // and syncs back
  });
});

describe("AI Console launcher — Close all + reclaim resources (CPE-442)", () => {
  it("closing a pane also tells the backend to reclaim that session's process", async () => {
    const posts: string[] = [];
    const { w } = await mountLauncher((path, opts) => {
      if (opts?.method === "POST") posts.push(path);
      return {};
    });
    w.addSession("s1", "Claude");
    w.addSession("s2", "Codex");
    expect(w.document.querySelectorAll(".tab").length).toBe(2);

    w.closeSession("s1");
    expect(posts).toContain("/api/session/s1/close"); // backend reclaim, not just a UI hide
    expect(w.document.querySelectorAll(".tab").length).toBe(1);
    expect(w.document.getElementById("close-all")).not.toBeNull(); // one session still open
  });

  it("'Close all' confirms, POSTs /api/close-all, clears every pane, and drops the button", async () => {
    const posts: string[] = [];
    const { w } = await mountLauncher((path, opts) => {
      if (opts?.method === "POST") posts.push(path);
      return {};
    });
    w.confirm = () => true;
    w.addSession("s1", "Claude");
    w.addSession("s2", "Codex");
    expect(w.document.getElementById("close-all")).not.toBeNull();

    await w.closeAllSessions();
    expect(posts).toContain("/api/close-all");
    expect(w.document.querySelectorAll(".tab").length).toBe(0); // every pane torn down
    expect(w.document.querySelectorAll(".term-pane").length).toBe(0);
    expect(w.document.getElementById("close-all")).toBeNull(); // strip empties → button gone
  });

  it("'Close all' cancelled at the confirm does nothing", async () => {
    const posts: string[] = [];
    const { w } = await mountLauncher((path, opts) => {
      if (opts?.method === "POST") posts.push(path);
      return {};
    });
    w.confirm = () => false;
    w.addSession("s1", "Claude");
    await w.closeAllSessions();
    expect(posts).not.toContain("/api/close-all");
    expect(w.document.querySelectorAll(".tab").length).toBe(1); // untouched
  });
});
