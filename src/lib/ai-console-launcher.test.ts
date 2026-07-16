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

describe("AI Console launcher — reattach tabs on reopen (CPE-461)", () => {
  it("recreates a tab for each still-running session on boot", async () => {
    const { w } = await mountLauncher((path) => {
      if (path === "/api/sessions") return { sessions: [{ id: "s7", name: "Claude · openrouter · sonnet" }] };
      return {};
    });
    // Boot's reattach is a few awaits deep — poll for the restored tab.
    for (let i = 0; i < 40 && !w.document.querySelector(".tab-label"); i++) await new Promise((r) => setTimeout(r, 5));
    const tabs = [...w.document.querySelectorAll(".tab-label")].map((e: any) => e.textContent);
    expect(tabs).toContain("Claude · openrouter · sonnet");
    expect(w.document.querySelectorAll(".term-pane").length).toBe(1);
  });

  it("boots with no tabs when nothing is running", async () => {
    const { w } = await mountLauncher((path) => (path === "/api/sessions" ? { sessions: [] } : {}));
    await new Promise((r) => setTimeout(r, 10));
    expect(w.document.querySelectorAll(".tab-label").length).toBe(0);
  });
});

describe("AI Console launcher — reseller keys in the Keys panel (CPE-452)", () => {
  it("offers resellers in the key dropdown and routes save to the reseller endpoint", async () => {
    const posts: { path: string; body: any }[] = [];
    const { w } = await mountLauncher((path, opts) => {
      if (opts?.method === "POST") posts.push({ path, body: JSON.parse(opts.body) });
      if (path === "/api/reseller-keys") return { resellers: [] };
      if (path === "/api/keys") return { credentials: [] };
      return {};
    });
    await w.openKeys();
    const opts = [...w.document.getElementById("key-provider").options].map((o: any) => o.value);
    expect(opts).toContain("reseller:openrouter"); // resellers appear alongside providers

    // Saving a reseller entry routes to /api/reseller-keys (not /api/keys).
    w.document.getElementById("key-provider").value = "reseller:openrouter";
    w.document.getElementById("key-value").value = "sk-or-xyz";
    await w.saveKey();
    const resellerPost = posts.find((p) => p.path === "/api/reseller-keys");
    expect(resellerPost?.body).toMatchObject({ reseller: "openrouter", key: "sk-or-xyz" });
    expect(posts.some((p) => p.path === "/api/keys")).toBe(false); // never the provider path
  });
});

describe("AI Console launcher — Model picker combobox (CPE-454/460)", () => {
  const catalog = {
    reseller: "openrouter",
    models: [
      { id: "anthropic/claude-3.5-sonnet", reseller: "openrouter", display_name: "Claude 3.5 Sonnet", context_length: 200000, pricing: { prompt: 0.000003 }, modalities: ["text"], moderated: true },
      { id: "openai/gpt-4o", reseller: "openrouter", display_name: "GPT-4o", context_length: 128000, pricing: { prompt: 0.0000025 }, modalities: ["text"], moderated: false },
    ],
  };

  it("themes the dropdown with system colors, not a hardcoded dark bg (CPE-463)", () => {
    // Regression guard for the "black rectangle": the menu must use Canvas/CanvasText so it's legible
    // on the light theme (the launcher themes with system colors, not a `--bg` var).
    expect(HTML).toMatch(/#model-menu\b[\s\S]*?background:\s*Canvas;\s*color:\s*CanvasText/);
    expect(HTML).not.toContain("#1e1e1e"); // the old hardcoded dark fallback is gone
  });

  it("opens a real dropdown of models, filters, and picks one into the field (CPE-460)", async () => {
    const { w } = await mountLauncher((path) => (path.startsWith("/api/models") ? catalog : {}));
    await w.populateModels();
    // A visible ▾ toggle + an anchored menu — not a bare input.
    expect(w.document.getElementById("model-toggle")).not.toBeNull();
    w.openModelMenu();
    let opts = [...w.document.querySelectorAll(".model-opt")].map((e: any) => e.textContent);
    expect(opts.length).toBe(2);
    expect(opts.join(" ")).toMatch(/GPT-4o/);
    // Type-to-filter narrows the list — the `input` event drives the filter (CPE-465).
    const modelInput = w.document.getElementById("model");
    modelInput.value = "gpt";
    modelInput.dispatchEvent(new w.Event("input"));
    const rows = [...w.document.querySelectorAll(".model-opt")];
    expect(rows.length).toBe(1);
    // Picking fills the field and closes the menu.
    rows[0].dispatchEvent(new w.MouseEvent("mousedown"));
    expect(w.document.getElementById("model").value).toBe("openai/gpt-4o");
    expect(w.document.getElementById("model-menu").hidden).toBe(true);
  });

  it("opening the menu shows the FULL list even when the field holds a committed model id (CPE-465)", async () => {
    // The reported bug: after applyLastUsed pre-fills the field with a full model id, opening the
    // menu filtered the list down to that single exact match — "only one model for openrouter".
    const { w } = await mountLauncher((path) => (path.startsWith("/api/models") ? catalog : {}));
    await w.populateModels();
    // Simulate a returning user whose field already carries a committed selection.
    w.document.getElementById("model").value = "openai/gpt-4o";
    w.openModelMenu();
    const rows = [...w.document.querySelectorAll(".model-opt")];
    expect(rows.length).toBe(2); // full list, NOT filtered to the one committed id
  });

  it("re-populates when the provider changes, and stays editable/custom-capable", async () => {
    let reseller = "";
    const { w } = await mountLauncher((path) => {
      if (path.startsWith("/api/models")) { reseller = new URLSearchParams(path.split("?")[1]).get("reseller") || ""; return catalog; }
      return {};
    });
    const p = w.document.getElementById("provider");
    p.innerHTML = '<option value="groq">groq</option>';
    p.value = "groq";
    p.dispatchEvent(new w.Event("change"));
    await new Promise((r) => setTimeout(r, 0));
    expect(reseller).toBe("groq"); // provider change drives the fetch — one uniform control
    // The field remains free-text, so a custom/unlisted id is never a dead end.
    w.document.getElementById("model").value = "my/custom-model";
    expect(w.document.getElementById("model").value).toBe("my/custom-model");
  });

  it("shows a visible error + Refresh when the model fetch fails (not a silent empty box)", async () => {
    const { w } = await mountLauncher((path) => (path.startsWith("/api/models") ? { ok: false, status: 502 } : {}));
    await w.populateModels();
    w.openModelMenu();
    const msg = w.document.querySelector(".model-msg");
    expect(msg?.textContent).toMatch(/Couldn't load models/i);
    expect(msg?.querySelector("button")?.textContent).toMatch(/Refresh/i);
    expect(w.document.getElementById("model").disabled).toBeFalsy(); // still editable
  });
});

describe("AI Console launcher — session tabs match the main window (CPE-466)", () => {
  // The main explorer tab bar (src/app.css .tab) is a rounded-top raised "folder" tab: 34px tall,
  // 6px top-corner radius, and the active tab lifted onto the page background with a 3-sided border
  // box-shadow. The console tabs must adopt the same treatment (using system colors).
  it("uses the raised rounded-top tab shape, not the old flat underline style", () => {
    expect(HTML).toMatch(/\.tab\s*\{[^}]*height:\s*34px/);
    expect(HTML).toMatch(/\.tab\s*\{[^}]*border-radius:\s*6px 6px 0 0/);
  });

  it("lifts the active tab onto the page background with a 3-sided border, like the main window", () => {
    expect(HTML).toMatch(/\.tab\.active\s*\{[^}]*background:\s*Canvas/);
    expect(HTML).toMatch(/\.tab\.active\s*\{[^}]*box-shadow:[^}]*var\(--line\)/);
  });

  it("close (×) hover is a neutral grey affordance, not a hardcoded red", () => {
    expect(HTML).toMatch(/\.tab-close\s*\{[^}]*border-radius:\s*4px/);
    expect(HTML).not.toMatch(/\.tab-close:hover\s*\{[^}]*#d05656/);
  });
});

describe("AI Console launcher — per-session usage/cost (CPE-311)", () => {
  it("formats cost-leading, then tokens, and blanks when nothing was reported", async () => {
    const { w } = await mountLauncher(() => ({}));
    expect(w.fmtUsage({ costUsd: 0.1234, inputTokens: 1200, outputTokens: 800 })).toBe("$0.123 · 2.0k tok");
    expect(w.fmtUsage({ costUsd: 0, inputTokens: 500, outputTokens: 250 })).toBe("750 tok");
    expect(w.fmtUsage({ costUsd: 2.5, inputTokens: 0, outputTokens: 0 })).toBe("$2.50");
    expect(w.fmtUsage({ costUsd: 0, inputTokens: 0, outputTokens: 0 })).toBe("");
    expect(w.fmtUsage(null)).toBe("");
  });

  it("paints each session's usage onto its tab badge + the status bar for the active one", async () => {
    const sessions = {
      sessions: [
        { id: "s1", name: "Claude", usage: { costUsd: 0.5, inputTokens: 1000, outputTokens: 500 } },
        { id: "s2", name: "Aider", usage: { costUsd: 0, inputTokens: 0, outputTokens: 0 } },
      ],
    };
    const { w } = await mountLauncher((path) => (path.startsWith("/api/sessions") ? sessions : {}));
    w.addSession("s1", "Claude");
    w.addSession("s2", "Aider");
    w.applyUsage(sessions);
    const badges = [...w.document.querySelectorAll(".tab-usage")].map((e: any) => e.textContent);
    expect(badges).toContain("$0.500 · 1.5k tok");
    expect(badges).toContain(""); // s2 reported nothing → empty badge (CSS hides it)
    // s2 is the last added, so it's active; its status-bar readout is blank.
    expect(w.document.getElementById("sb-usage").textContent).toBe("");
    w.activate("s1");
    w.applyUsage(sessions);
    expect(w.document.getElementById("sb-usage").textContent).toBe("$0.500 · 1.5k tok");
  });
});

describe("AI Console launcher — reseller providers in the dropdown (CPE-469)", () => {
  it("offers every reseller whose protocol the agent speaks, as an extra provider option", async () => {
    const { w } = await mountLauncher();
    // A catalog: an openai-protocol agent + two resellers (one openai, one anthropic).
    const cat = {
      agents: [{ id: "qwen", name: "Qwen Code", installed: true, providers: ["native"], resellerProtocols: ["openai"], defaultModel: "qwen3" }],
      cwd: "/repo",
      resellers: [
        { id: "groq", name: "Groq", protocol: "openai" },
        { id: "together", name: "Together AI", protocol: "openai" },
        { id: "some-anthropic", name: "AnthropicOnly", protocol: "anthropic" },
      ],
      presets: { agents: { qwen: {} }, credentials: [] },
    };
    w.eval(`catalog = ${JSON.stringify(cat)}`);
    w.document.getElementById("agent").innerHTML = '<option value="qwen">Qwen Code</option>';
    w.document.getElementById("agent").value = "qwen";
    w.renderProviders();
    const opts = [...w.document.getElementById("provider").options].map((o: any) => ({ v: o.value, t: o.textContent }));
    // native + the two openai resellers; NOT the anthropic-only one (qwen doesn't speak anthropic).
    expect(opts.map((o) => o.v)).toEqual(["native", "groq", "together"]);
    expect(opts.find((o) => o.v === "groq")?.t).toBe("Groq (reseller)");
    expect(opts.some((o) => o.v === "some-anthropic")).toBe(false);
  });
});

describe("AI Console launcher — busy/wait cursor (CPE-482)", () => {
  it("maps body.busy to the OS progress cursor", () => {
    expect(HTML).toMatch(/body\.busy[\s\S]*?cursor:\s*progress\s*!important/);
  });

  it("toggles body.busy after the debounce and clears when the last op ends", async () => {
    const { w } = await mountLauncher(() => ({}));
    const end = w.beginBusy();
    expect(w.document.body.classList.contains("busy")).toBe(false); // under the 150ms debounce
    await new Promise((r) => setTimeout(r, 180));
    expect(w.document.body.classList.contains("busy")).toBe(true);
    end();
    expect(w.document.body.classList.contains("busy")).toBe(false);
  });
});
