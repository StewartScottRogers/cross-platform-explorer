/**
 * Agent Deck launcher UI harness (CPE-388).
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

describe("Agent Deck launcher — Keys panel error hints (CPE-386)", () => {
  it("turns a 'Secrets not granted' error into an actionable message; passes others through", async () => {
    const { w } = await mountLauncher();
    expect(w.permHint("Secrets is not granted to 'ai-console'")).toMatch(/reopen the Agent Deck/i);
    expect(w.permHint("secrets denied")).toMatch(/Allow for Secrets/i);
    expect(w.permHint("bad json: x")).toBe("bad json: x");
  });
});

describe("Agent Deck launcher — Agent Grid view (CPE-506)", () => {
  it("gridDims gives a near-square, cols-first best fit that reflows", async () => {
    const { w } = await mountLauncher();
    expect(w.gridDims(1)).toEqual({ rows: 1, cols: 1 });
    expect(w.gridDims(2)).toEqual({ rows: 1, cols: 2 }); // side by side
    expect(w.gridDims(3)).toEqual({ rows: 2, cols: 2 });
    expect(w.gridDims(4)).toEqual({ rows: 2, cols: 2 });
    expect(w.gridDims(5)).toEqual({ rows: 2, cols: 3 }); // reflows to 2×3
    expect(w.gridDims(9)).toEqual({ rows: 3, cols: 3 });
    expect(w.gridDims(16)).toEqual({ rows: 4, cols: 4 });
  });

  it("toggles between tabs (one pane visible) and grid (all tiles visible)", async () => {
    const { w } = await mountLauncher();
    const doc = w.document;
    w.addSession("agent-1", "A");
    w.addSession("agent-2", "B");
    const terms = doc.getElementById("terms");
    const panes = () => [...doc.querySelectorAll(".term-pane")];

    // Tabs (default): only the active (last-added) pane is shown.
    expect(terms.classList.contains("grid-view")).toBe(false);
    expect(panes().filter((p: any) => p.style.display !== "none").length).toBe(1);

    // Grid: every pane visible, columns from gridDims(2).
    w.toggleView();
    expect(terms.classList.contains("grid-view")).toBe(true);
    expect(terms.style.getPropertyValue("--grid-cols")).toBe("2");
    expect(panes().every((p: any) => p.style.display !== "none")).toBe(true);
    expect(panes().filter((p: any) => p.classList.contains("focused")).length).toBe(1); // one focused tile

    // Back to tabs: single pane again.
    w.toggleView();
    expect(terms.classList.contains("grid-view")).toBe(false);
    expect(panes().filter((p: any) => p.style.display !== "none").length).toBe(1);
  });

  it("shows the view toggle only when sessions exist, and reflows columns as they change", async () => {
    const { w } = await mountLauncher();
    const doc = w.document;
    const bar = doc.getElementById("view-bar");
    const terms = doc.getElementById("terms");
    expect(bar.style.display).toBe("none"); // no sessions yet

    w.addSession("a", "A"); w.addSession("b", "B"); w.addSession("c", "C");
    expect(bar.style.display).toBe("flex");
    w.setView("grid");
    expect(terms.style.getPropertyValue("--grid-cols")).toBe("2"); // 3 → 2×2

    w.addSession("d", "D"); w.addSession("e", "E");
    expect(terms.style.getPropertyValue("--grid-cols")).toBe("3"); // 5 → 2×3
  });
});

describe("Agent Deck launcher — boot loader (CPE-561, supersedes CPE-552)", () => {
  it("ships an animated boot overlay + spinner + label (pre-JS coverage)", () => {
    expect(HTML).toMatch(/<div id="boot-overlay"[^>]*class="boot-overlay"/);
    expect(HTML).toMatch(/class="boot-ring"/);
    expect(HTML).toMatch(/\.boot-ring[\s\S]*?animation:\s*boot-spin/);
    expect(HTML).toMatch(/Starting the Agent Deck/);
    // The ugly progress-cursor boot rule is gone (the overlay is the indicator now).
    expect(HTML).not.toMatch(/body\.booting[\s\S]*?cursor:\s*progress/);
  });

  it("endBoot() fades the overlay out (marks it done) and clears the booting class", async () => {
    const { w } = await mountLauncher();
    // Mount already ran endBoot once; clear its overlay and set up a fresh boot state.
    w.document.getElementById("boot-overlay")?.remove();
    const o = w.document.createElement("div");
    o.id = "boot-overlay";
    w.document.body.appendChild(o);
    w.document.body.classList.add("booting");
    w.endBoot();
    expect(w.document.body.classList.contains("booting")).toBe(false);
    expect(o.classList.contains("done")).toBe(true);
  });

  it("boot dismisses the overlay once the launcher has loaded", async () => {
    const { w } = await mountLauncher(); // awaits boot settle → endBoot ran
    const o = w.document.getElementById("boot-overlay");
    // Either already removed, or faded (has the `done` class) pending its removal timer.
    expect(o === null || o.classList.contains("done")).toBe(true);
    expect(w.document.body.classList.contains("booting")).toBe(false);
  });
});

describe("Agent Deck launcher — live swarm trigger (CPE-541)", () => {
  it("runSwarm POSTs the mission to /api/swarm/run and reports success", async () => {
    const calls: Array<{ path: string; body: any }> = [];
    const { w } = await mountLauncher((path, opts) => {
      if (path === "/api/swarm/run") {
        calls.push({ path, body: JSON.parse(opts.body) });
        return { ok: true, data: { ok: true, mission: "/tmp/cpe-swarm-1" } };
      }
      return {};
    });
    const mission = {
      team: { name: "swarm", description: "", roles: [{ role: "coordinator", agent: "claude", count: 1 }, { role: "builder", agent: "claude", count: 1 }] },
      tasks: [{ id: "t1", description: "add tests", role: "builder", globs: ["**"], gate: "none" }],
      provider: "openrouter",
    };
    const ok = await w.runSwarm(mission);
    expect(ok).toBe(true);
    expect(calls).toHaveLength(1);
    expect(calls[0].body.tasks[0].description).toBe("add tests");
    expect(calls[0].body.team.roles).toHaveLength(2);
  });

  it("runSwarm refuses a mission with no tasks and does not call the backend", async () => {
    const calls: string[] = [];
    const { w } = await mountLauncher((path) => {
      if (path === "/api/swarm/run") calls.push(path);
      return {};
    });
    const ok = await w.runSwarm({ team: { name: "x", description: "", roles: [] }, tasks: [] });
    expect(ok).toBe(false);
    expect(calls).toHaveLength(0);
  });

  it("the Run swarm button reveals the task field first, then runs on the second trigger", async () => {
    const posted: any[] = [];
    const { w } = await mountLauncher((path, opts) => {
      if (path === "/api/swarm/run") { posted.push(JSON.parse(opts.body)); return { ok: true, data: { ok: true } }; }
      return {};
    });
    const doc = w.document;
    expect(doc.getElementById("swarm-row").hidden).toBe(true);
    await w.runSwarmFromForm(); // first: reveal
    expect(doc.getElementById("swarm-row").hidden).toBe(false);
    expect(posted).toHaveLength(0);

    doc.getElementById("swarm-task").value = "build the parser";
    await w.runSwarmFromForm(); // second: run
    expect(posted).toHaveLength(1);
    expect(posted[0].tasks[0].description).toBe("build the parser");
    expect(posted[0].provider).toBeDefined();
  });

  it("Load demo reveals + pre-fills the swarm form for the selected demo, then Start runs it (CPE-924/925)", async () => {
    const posted: any[] = [];
    const { w } = await mountLauncher((path, opts) => {
      if (path === "/api/swarm/run") { posted.push(JSON.parse(opts.body)); return { ok: true, data: { ok: true } }; }
      return {};
    });
    const doc = w.document;
    expect(doc.getElementById("swarm-row").hidden).toBe(true);
    // Default demo (first option = "hello"): reveals the field, fills the ready-made tasks, runs nothing yet.
    w.demoSwarm();
    expect(doc.getElementById("swarm-row").hidden).toBe(false);
    expect(doc.getElementById("swarm-task").value).toContain("README-DEMO.md");
    expect(posted).toHaveLength(0);
    // Pressing Start launches the pre-filled demo — two disjoint, parallel builder tasks.
    await w.runSwarmFromForm();
    expect(posted).toHaveLength(1);
    expect(posted[0].tasks.map((t: any) => t.globs[0])).toEqual(["README-DEMO.md", "NOTES-DEMO.md"]);
    expect(posted[0].team.roles.find((r: any) => r.role === "builder").count).toBe(2);
  });

  it("the demo dropdown offers ≥4 types, and each loads its own safe tasks (CPE-925)", async () => {
    const { w } = await mountLauncher();
    const doc = w.document;
    const select = doc.getElementById("demo-select") as HTMLSelectElement;
    expect(select.options.length).toBeGreaterThanOrEqual(4);

    // Picking "tidy" loads a single, non-destructive proposal task.
    select.value = "tidy";
    w.demoSwarm();
    const tidy = doc.getElementById("swarm-task").value;
    expect(tidy).toContain("CLEANUP-PLAN.md");
    expect(tidy.toLowerCase()).toContain("not"); // proposes but does NOT perform
    expect(w.parseSwarmTasks(tidy)).toHaveLength(1);

    // Changing the dropdown reloads the tasks (change handler), no re-click needed.
    select.value = "docs";
    select.dispatchEvent(new w.Event("change"));
    expect(w.parseSwarmTasks(doc.getElementById("swarm-task").value).map((t: any) => t.globs[0]))
      .toEqual(["DEMO-README.md", "DEMO-CONTRIBUTING.md"]);
  });

  it("keeps the demo controls inside the swarm pane: Run swarm drops the panel, then Load demo shows the picker (CPE-931/932)", async () => {
    const { w } = await mountLauncher();
    const doc = w.document;
    const picker = doc.getElementById("demo-picker");
    // Both the "Load a demo" trigger AND the dropdown live inside the swarm row (the pane), not the toolbar.
    expect(doc.getElementById("swarm-demo").closest("#swarm-row")).toBeTruthy();
    expect(doc.getElementById("demo-select").closest("#swarm-row")).toBeTruthy();
    // The Load-a-demo trigger is NOT in the top toolbar button row (it moved into the swarm pane).
    const topRow = doc.querySelector(".toolbar > .row");
    expect(topRow?.contains(doc.getElementById("swarm-demo"))).toBe(false);
    // Everything hidden until the panel drops.
    expect(doc.getElementById("swarm-row").hidden).toBe(true);
    expect(picker.hidden).toBe(true);

    // "Run swarm" drops the panel (revealing the Load-a-demo trigger) but NOT the demo picker.
    w.runSwarmFromForm();
    expect(doc.getElementById("swarm-row").hidden).toBe(false);
    expect(picker.hidden).toBe(true);

    // "Load a demo" reveals the demo picker and loads the default demo.
    w.demoSwarm();
    expect(picker.hidden).toBe(false);
    expect(doc.getElementById("swarm-task").value).toContain("README-DEMO.md");
  });

  it("staffs one builder per task line — two lines → two builders + two tasks", async () => {
    const posted: any[] = [];
    const { w } = await mountLauncher((path, opts) => {
      if (path === "/api/swarm/run") { posted.push(JSON.parse(opts.body)); return { ok: true, data: { ok: true } }; }
      return {};
    });
    const doc = w.document;
    await w.runSwarmFromForm(); // reveal
    doc.getElementById("swarm-task").value = "add tests\ndocument it\n";
    await w.runSwarmFromForm(); // run
    expect(posted).toHaveLength(1);
    const m = posted[0];
    expect(m.tasks.map((t: any) => t.description)).toEqual(["add tests", "document it"]);
    expect(m.tasks.map((t: any) => t.id)).toEqual(["t1", "t2"]);
    const builder = m.team.roles.find((r: any) => r.role === "builder");
    expect(builder.count).toBe(2); // one builder per task, so they can run concurrently
    expect(m.tasks.every((t: any) => t.role === "builder")).toBe(true);
  });

  it("parses an optional 'glob :: task' scope so disjoint tasks can run in parallel", () => {
    // parseSwarmTasks is a pure top-level helper — assert its shape directly.
    return mountLauncher().then(({ w }) => {
      const tasks = w.parseSwarmTasks("src/**,lib/** :: refactor\ndocs/** :: write docs\nplain task\n");
      expect(tasks).toHaveLength(3);
      expect(tasks[0]).toMatchObject({ id: "t1", description: "refactor", globs: ["src/**", "lib/**"] });
      expect(tasks[1]).toMatchObject({ id: "t2", description: "write docs", globs: ["docs/**"] });
      expect(tasks[2]).toMatchObject({ description: "plain task", globs: ["**"] }); // no scope → whole tree
      expect(w.parseSwarmTasks("   \n\n")).toEqual([]); // blank input → no tasks
    });
  });

  it("opens the coordination panel and renders mailbox + memory from /api/swarm/activity (CPE-592)", async () => {
    const { w } = await mountLauncher((path) => {
      if (path === "/api/swarm/run") return { data: { ok: true, mission: "C:\\Temp\\cpe-swarm-42" } };
      if (path.startsWith("/api/swarm/activity")) {
        return { data: { ok: true,
          mailbox: [{ from: "claude#builder1", to: "broadcast", kind: "done", body: "builder1 done" }],
          memory: [{ id: "note-1", tags: ["greeting"], body: "Hello from builder1" }] } };
      }
      return {};
    });
    const doc = w.document;
    expect(doc.getElementById("swarm-panel").hidden).toBe(true);
    const mission = {
      team: { name: "s", description: "", roles: [{ role: "builder", agent: "claude", count: 1 }] },
      tasks: [{ id: "t1", description: "x", role: "builder", globs: ["**"], gate: "none" }],
      provider: "native",
    };
    await w.runSwarm(mission);
    expect(doc.getElementById("swarm-panel").hidden).toBe(false); // panel opened for the mission
    await w.pollSwarmActivity();
    expect(doc.getElementById("sw-mailbox").textContent).toContain("builder1 done");
    expect(doc.getElementById("sw-mailbox").textContent).toContain("done"); // the kind pill
    expect(doc.getElementById("sw-memory").textContent).toContain("Hello from builder1");
    w.closeSwarmPanel(); // stop the poll timer so it doesn't leak past the test
    expect(doc.getElementById("swarm-panel").hidden).toBe(true);
  });

  it("adopts a server-created swarm session into a tab on poll, from a fresh console (CPE-586)", async () => {
    let sessionsResp: any = { sessions: [] }; // empty at boot; a swarm launches afterwards
    const { w } = await mountLauncher((path) => {
      if (path === "/api/sessions") return { data: sessionsResp };
      return {};
    });
    const doc = w.document;
    const panes = () => doc.querySelectorAll(".term-pane").length;
    expect(panes()).toBe(0); // fresh console — the old poll early-returned here and never adopted

    // A swarm run creates the session server-side (no client /api/launch); the poll must pick it up.
    sessionsResp = { sessions: [{ id: "t1", name: "claude#builder1", usage: {} }] };
    await w.refreshUsage();
    expect(panes()).toBe(1); // the backend-created swarm session became a tab
  });
});

describe("Agent Deck launcher — grid pane identity + keyboard nav (CPE-507)", () => {
  it("nextPaneId moves row-major in a cols-wide grid and clamps at edges", async () => {
    const { w } = await mountLauncher();
    const ids = ["a", "b", "c", "d", "e"]; // 5 panes → gridDims cols = 3 (rows 2×3)
    // 0 1 2
    // 3 4
    expect(w.nextPaneId(ids, "a", "right", 3)).toBe("b");
    expect(w.nextPaneId(ids, "a", "down", 3)).toBe("d");
    expect(w.nextPaneId(ids, "e", "left", 3)).toBe("d");
    expect(w.nextPaneId(ids, "d", "up", 3)).toBe("a");
    // Edge clamps: no movement past the ends / off-grid.
    expect(w.nextPaneId(ids, "a", "left", 3)).toBe("a");
    expect(w.nextPaneId(ids, "a", "up", 3)).toBe("a");
    expect(w.nextPaneId(ids, "e", "down", 3)).toBe("e");
    expect(w.nextPaneId(ids, "x", "right", 3)).toBe("x"); // unknown id unchanged
  });

  it("gives every grid tile an identity header with the CPE-490 chip number", async () => {
    const { w } = await mountLauncher();
    const doc = w.document;
    w.addSession("agent-1", "Claude Code");
    w.addSession("agent-2", "aider");
    const heads = [...doc.querySelectorAll(".pane-head")];
    expect(heads.length).toBe(2);
    // Chip number is derived from the id digits (sessionNum), label is the session name.
    const chip = heads[0].querySelector(".pane-chip");
    const label = heads[0].querySelector(".pane-label");
    expect(chip.textContent).toBe("1");
    expect(label.textContent).toBe("Claude Code");
  });

  it("focusPane moves the focus ring + active highlight to the clicked tile", async () => {
    const { w } = await mountLauncher();
    const doc = w.document;
    w.addSession("agent-1", "A");
    w.addSession("agent-2", "B");
    w.setView("grid");
    w.focusPane("agent-1");
    const panes = [...doc.querySelectorAll(".term-pane")];
    const focused = panes.filter((p: any) => p.classList.contains("focused"));
    expect(focused.length).toBe(1);
    // The focused pane is the first tile (agent-1), matching the active tab.
    const activeTabs = [...doc.querySelectorAll(".tab.active")];
    expect(activeTabs.length).toBe(1);
  });
});

describe("Agent Deck launcher — 16-pane throttling (CPE-508)", () => {
  it("paneWritePolicy: focused writes now, tiles coalesce, hidden defers", async () => {
    const { w } = await mountLauncher();
    expect(w.paneWritePolicy("focused")).toBe("sync");
    expect(w.paneWritePolicy("tile")).toBe("throttled");
    expect(w.paneWritePolicy("hidden")).toBe("deferred");
  });

  it("caps visible grid tiles at 16, always including the focused agent", async () => {
    const { w } = await mountLauncher();
    for (let i = 1; i <= 20; i++) w.addSession("agent-" + i, "A" + i);
    const vis = w.visibleGridIds(); // active is agent-20 (last added), past the first 16
    expect(vis.size).toBe(17); // first 16 by order + the focused one
    expect(vis.has("agent-1")).toBe(true);
    expect(vis.has("agent-20")).toBe(true); // focused is never hidden
    expect(vis.has("agent-17")).toBe(false); // past the cap + not focused → reachable via tabs
  });

  it("buffers output for a hidden pane and flushes it when shown — no output lost", async () => {
    const { w } = await mountLauncher();
    w.addSession("a", "A");
    w.addSession("b", "B"); // b is active; a is hidden in tabs view
    w.eval("routeWrite(sessions.get('a'), 'hello')");
    expect(w.eval("sessions.get('a')._buf.length")).toBe(1); // deferred, buffered not written
    w.activate("a"); // shows a → flushes its buffer
    expect(w.eval("sessions.get('a')._buf.length")).toBe(0);
  });
});

describe("Agent Deck launcher — grid layout persistence (CPE-509)", () => {
  it("serializeLayout/parseLayout round-trip and tolerate garbage", async () => {
    const { w } = await mountLauncher();
    const s = w.serializeLayout({ viewMode: "grid", activeId: "a" });
    expect(w.parseLayout(s)).toEqual({ viewMode: "grid", activeId: "a" });
    expect(w.parseLayout("not json")).toEqual({ viewMode: "tabs", activeId: null });
    expect(w.parseLayout(JSON.stringify({ viewMode: "weird" }))).toEqual({ viewMode: "tabs", activeId: null });
  });

  it("persists the view per workspace and restores it on reattach", async () => {
    const { w } = await mountLauncher();
    w.addSession("a", "A");
    w.setView("grid"); // user switches to grid → saved for this cwd
    w.eval("viewMode = 'tabs'"); // simulate a fresh relaunch defaulting to tabs
    w.restoreLayout(); // sessions exist → reapply saved view
    expect(w.eval("viewMode")).toBe("grid");
  });

  it("scopes layout by workspace so unrelated folders don't clobber", async () => {
    const { w } = await mountLauncher();
    w.addSession("a", "A");
    w.setView("grid"); // saved under cwd "/repo"
    w.eval("catalog.cwd = '/other-project'");
    expect(w.loadLayout().viewMode).toBe("tabs"); // a different workspace defaults to tabs
  });

  it("resets to the default tabs view when all sessions close", async () => {
    const { w } = await mountLauncher();
    w.addSession("a", "A");
    w.setView("grid");
    w.confirm = () => true; // closeAllSessions confirms first
    await w.closeAllSessions();
    expect(w.eval("viewMode")).toBe("tabs");
    expect(w.loadLayout().viewMode).toBe("tabs");
  });
});

describe("Agent Deck launcher — responsive grid columns (CPE-510)", () => {
  it("colsForWidth caps columns so tiles never drop below the legible minimum", async () => {
    const { w } = await mountLauncher();
    // Wide enough for the near-square ideal.
    expect(w.colsForWidth(4, 1000, 320)).toBe(2); // ideal 2, fits 3 → 2
    expect(w.colsForWidth(9, 1000, 320)).toBe(3); // ideal 3, fits 3 → 3
    // Narrow: fewer columns than ideal so tiles stay ≥ 320px.
    expect(w.colsForWidth(4, 700, 320)).toBe(2); // fits 2
    expect(w.colsForWidth(4, 400, 320)).toBe(1); // fits 1 → collapses to a single column
    expect(w.colsForWidth(9, 700, 320)).toBe(2); // ideal 3 but only 2 fit
    // Very narrow always yields at least one column (no zero).
    expect(w.colsForWidth(6, 100, 320)).toBe(1);
  });

  it("treats unknown/zero width (headless) as wide → uses the ideal columns", async () => {
    const { w } = await mountLauncher();
    expect(w.colsForWidth(4, 0, 320)).toBe(2);
    expect(w.colsForWidth(5, 0, 320)).toBe(3);
  });
});

describe("Agent Deck launcher — agent-state awareness (CPE-512)", () => {
  it("agentStateFromText classifies blocked / done / working / idle by priority", async () => {
    const { w } = await mountLauncher();
    // Blocked: an input prompt awaiting the user (outranks everything).
    expect(w.agentStateFromText("Building...\nDo you want to continue? (y/n)")).toBe("blocked");
    expect(w.agentStateFromText("Overwrite file? [Y/n]")).toBe("blocked");
    expect(w.agentStateFromText("What should I call the module?")).toBe("blocked"); // ends with ?
    // Done: a completion marker at the end.
    expect(w.agentStateFromText("running tests\nAll tests passed. Done.")).toBe("done");
    expect(w.agentStateFromText("wrote 3 files\n✓")).toBe("done");
    // Working: spinner / ellipsis / activity verb.
    expect(w.agentStateFromText("⠹ compiling crate")).toBe("working");
    expect(w.agentStateFromText("Thinking...")).toBe("working");
    expect(w.agentStateFromText("Generating a plan")).toBe("working");
    // Idle: a plain shell prompt / nothing notable.
    expect(w.agentStateFromText("$ ")).toBe("idle");
    expect(w.agentStateFromText("")).toBe("idle");
  });

  it("sessionState layers recency: a fresh write reads as working, a prompt still wins", async () => {
    const { w } = await mountLauncher();
    const now = 1_000_000;
    // Recent, non-prompt output → working even if the text alone looks idle.
    expect(w.sessionState({ _recent: "some log line", _lastOut: now }, now + 200)).toBe("working");
    // Gone quiet → falls back to the text classification (idle here).
    expect(w.sessionState({ _recent: "$ ", _lastOut: now }, now + 5000)).toBe("idle");
    // A prompt outranks recency — still blocked even with a fresh write.
    expect(w.sessionState({ _recent: "Proceed? (y/n)", _lastOut: now }, now + 100)).toBe("blocked");
  });

  it("selectRequestedSession activates the requested tab, ignores unknown/blank (CPE-532)", async () => {
    const { w } = await mountLauncher();
    w.addSession("agent-1", "A");
    w.addSession("agent-2", "B"); // agent-2 active (last added)
    w.selectRequestedSession("agent-1");
    expect(w.eval("activeId")).toBe("agent-1");
    // Unknown id + blank are no-ops (stay on agent-1).
    w.selectRequestedSession("ghost");
    w.selectRequestedSession(null);
    expect(w.eval("activeId")).toBe("agent-1");
  });

  it("shows a state dot on both the tab and the grid pane header", async () => {
    const { w } = await mountLauncher();
    const doc = w.document;
    w.addSession("agent-1", "A");
    expect(doc.querySelectorAll(".tab .state-dot").length).toBe(1);
    expect(doc.querySelectorAll(".pane-head .state-dot").length).toBe(1);
    // A blocked prompt marks the tab and pulses.
    w.eval("const s = sessions.get('agent-1'); s._recent = 'Continue? (y/n)'; renderState(s);");
    expect(doc.querySelector(".tab").classList.contains("blocked")).toBe(true);
  });
});

describe("Agent Deck launcher — named sets / presets (the reported confusion)", () => {
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

describe("Agent Deck launcher — catalog controls", () => {
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

describe("Agent Deck launcher — Help panel + Manage menu (CPE-390)", () => {
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

describe("Agent Deck launcher — inexperienced-user goal (CPE-392/393/394)", () => {
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

describe("Agent Deck launcher — Browse ↔ Project folder sync (CPE-395)", () => {
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

describe("Agent Deck launcher — Close all + reclaim resources (CPE-442)", () => {
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

describe("Agent Deck launcher — reattach tabs on reopen (CPE-461)", () => {
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

describe("Agent Deck launcher — reseller keys in the Keys panel (CPE-452)", () => {
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

describe("Agent Deck launcher — Model picker combobox (CPE-454/460)", () => {
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

describe("Agent Deck launcher — session tabs match the main window (CPE-466)", () => {
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

describe("Agent Deck launcher — per-session usage/cost (CPE-311)", () => {
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

describe("Agent Deck launcher — reseller providers in the dropdown (CPE-469)", () => {
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

describe("Agent Deck launcher — busy/wait cursor (CPE-482)", () => {
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

describe("Agent Deck launcher — area '?' opens the regular Documents dialog (CPE-929)", () => {
  it("swarm '?' emits open-docs for the swarms section instead of the inline panel", async () => {
    const { w } = await mountLauncher();
    const emit = vi.fn();
    w.__TAURI__ = { event: { emit } };
    w.document.getElementById("swarm-help").click();
    expect(emit).toHaveBeenCalledWith("open-docs", { slug: "09-swarms" });
    // The inline help overlay stays closed — it went to the real docs, not the panel.
    expect(w.document.getElementById("help-overlay").hidden).toBe(true);
  });

  it("grid '?' emits open-docs for the agent-grid section", async () => {
    const { w } = await mountLauncher();
    const emit = vi.fn();
    w.__TAURI__ = { event: { emit } };
    w.document.getElementById("grid-help").click();
    expect(emit).toHaveBeenCalledWith("open-docs", { slug: "05-agent-grid" });
  });

  it("falls back to a FOCUSED inline topic when Tauri events are unavailable (CPE-930)", async () => {
    const { w } = await mountLauncher();
    const doc = w.document;
    expect(doc.getElementById("help-overlay").hidden).toBe(true);
    doc.getElementById("swarm-help").click(); // no __TAURI__ → focused inline topic
    expect(doc.getElementById("help-overlay").hidden).toBe(false);
    // Only the swarm topic shows, titled "Swarms" — not the whole manual.
    expect(doc.getElementById("help-title").textContent).toBe("Swarms");
    const shown = (t: string) => !doc.querySelector(`.help-topic[data-topic="${t}"]`).hidden;
    expect(shown("swarm")).toBe(true);
    expect(shown("deck")).toBe(false);
    expect(shown("grid")).toBe(false);
  });

  it("the top-bar '?' shows the full guide with every topic (CPE-930)", async () => {
    const { w } = await mountLauncher();
    const doc = w.document;
    doc.getElementById("help-btn").click();
    expect(doc.getElementById("help-title").textContent).toBe("How the Agent Deck works");
    const shown = (t: string) => !doc.querySelector(`.help-topic[data-topic="${t}"]`).hidden;
    expect(shown("deck") && shown("grid") && shown("swarm")).toBe(true);
  });
});
