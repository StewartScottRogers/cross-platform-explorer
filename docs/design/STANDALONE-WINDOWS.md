# Standalone windows — one bundle, many surfaces

_How the app renders secondary windows (torn-off preview, Agent Board) from the **same** frontend
bundle, and the one security distinction that matters between them._

## The pattern

Every window loads the same `index.html`; a **URL query marker** picks which surface it mounts, so a
secondary window shows only its own UI with no explorer chrome. The decision is a pure function
(`src/lib/bootMode.ts`, unit-tested) consumed by `src/main.ts`:

| Marker | Surface | Root component |
|--------|---------|----------------|
| `?float=1` | torn-off tabbed preview (CPE-234) | `FloatPreview` |
| `?board=1` | standalone Agent Board (CPE-841) | `AgentBoardApp` → `BoardView windowed` |
| _(none)_ | the full file explorer | `App` |

```
main.ts:  bootMode(location.search)  →  "float" | "board" | "explorer"  →  mount the matching root
```

A window is created with the Tauri webview API, keyed by a **fixed label** so it is an **app-wide
singleton** — a second launch focuses the existing window instead of opening another:

```ts
const existing = await WebviewWindow.getByLabel("agent-board");
if (existing) { await existing.setFocus(); return; }
new WebviewWindow("agent-board", { url: "index.html?board=1", title: "Agent Board", resizable: true, /* … */ });
```

Size and position persist automatically via `tauri-plugin-window-state` (keyed by label). A component
that is normally an in-app overlay takes a `windowed` prop so it fills the window (no dim backdrop, no
centred panel) when it *is* the window — see `BoardView`'s `windowed`.

## The one security distinction: trusted vs isolated

Both the **AI Console** window and the **Agent Board** window are singleton `WebviewWindow`s created the
same way — but they sit on opposite sides of the trust boundary, and that shows up in
`src-tauri/capabilities/default.json`'s `windows` list:

| Window | Loads | In a capability? | Tauri API (`invoke`)? |
|--------|-------|------------------|------------------------|
| **Agent Board** (`agent-board`) | our **own** trusted `BoardView` (bundled frontend) | **yes** — listed in `default.json` | **yes** — it needs to `invoke` `ticket_board` to read/move cards |
| **AI Console** (`ai-console`) | the **untrusted** sidecar's loopback URL | **no** — in no capability by design | **no** — the untrusted UI is denied all Tauri APIs |

So: a window that renders our own code and must call the backend is **added to a capability**; a window
that hosts untrusted content is **kept out of every capability** so it cannot reach the Tauri API. Adding
a new trusted window means adding its label to `windows` in `default.json` — omit that and its `invoke`
calls are denied at runtime.

## Related

- Boot-mode selection: `src/lib/bootMode.ts` (+ `bootMode.test.ts`).
- Agent Board window: `openAgentBoard()` in `src/App.svelte`; `AgentBoardApp.svelte`; `BoardView`'s
  `windowed` prop (CPE-841 / CPE-843 / CPE-844).
- Capabilities model: [Tauri v2 capabilities](https://v2.tauri.app/security/capabilities/).
