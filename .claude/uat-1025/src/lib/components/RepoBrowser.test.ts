/**
 * RepoBrowser render test (CPE-434/435/436/439) — browse a forge repo's tree in-app, clone it, and
 * remember the token. Uses a command-router `invoke` mock (the component also calls forge_get_token
 * on mount and forge_set/delete_token on Remember, so a sequential mock would mis-align).
 */
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
import RepoBrowser from "./RepoBrowser.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const openMock = openFolderDialog as unknown as ReturnType<typeof vi.fn>;

const root = [
  { name: "src", path: "src", is_dir: true, size: 0 },
  { name: "README.md", path: "README.md", is_dir: false, size: 1024 },
];
const srcDir = [{ name: "lib.rs", path: "src/lib.rs", is_dir: false, size: 42 }];

/** Configure the invoke router. `browse` maps a path → entries (or throws if an Error); `savedToken`
 *  is what forge_get_token returns on mount. Records forge_set_token/forge_clone calls. */
function route(opts: { browse?: (path: string) => any; clone?: any; savedToken?: string | null }) {
  const calls: { cmd: string; args: any }[] = [];
  invokeMock.mockImplementation(async (cmd: string, args: any) => {
    calls.push({ cmd, args });
    if (cmd === "forge_get_token") return opts.savedToken ?? null;
    if (cmd === "forge_set_token" || cmd === "forge_delete_token") return undefined;
    if (cmd === "forge_browse") {
      const r = opts.browse ? opts.browse(args.path ?? "") : [];
      if (r instanceof Error) throw r;
      return r;
    }
    if (cmd === "forge_clone") {
      if (opts.clone instanceof Error) throw opts.clone;
      return opts.clone ?? "ok";
    }
    return undefined;
  });
  return calls;
}

beforeEach(() => { invokeMock.mockReset(); openMock.mockReset(); });

describe("RepoBrowser", () => {
  it("browses a repo and calls forge_browse with owner/name + provider", async () => {
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider: "github", repo: "tauri-apps/tauri" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));

    await waitFor(() => expect(calls.some((c) => c.cmd === "forge_browse" && c.args.repo === "tauri-apps/tauri" && c.args.path === "")).toBe(true));
    expect(await screen.findByText("src")).toBeTruthy();
    expect(screen.getByText("README.md")).toBeTruthy();
  });

  it("navigates into a folder and back up", async () => {
    route({ browse: (p) => (p === "src" ? srcDir : root) });
    render(RepoBrowser, { props: { repo: "o/r" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    await screen.findByText("src");
    await fireEvent.click(screen.getByText("src"));
    expect(await screen.findByText("lib.rs")).toBeTruthy();
    expect(screen.getByText("..")).toBeTruthy();
  });

  it("rejects a bare name without owner/, without hitting the backend", async () => {
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { repo: "justaname" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    expect(calls.some((c) => c.cmd === "forge_browse")).toBe(false);
    expect(await screen.findByText(/owner\/name/i)).toBeTruthy();
  });

  it("clones into <chosen>/<repo-name> via forge_clone after a folder pick", async () => {
    openMock.mockResolvedValueOnce("/home/me/code");
    const calls = route({ clone: "ok" });
    render(RepoBrowser, { props: { provider: "github", repo: "tauri-apps/tauri" } });
    await fireEvent.click(screen.getByRole("button", { name: "Clone" }));

    await waitFor(() => expect(calls.some((c) => c.cmd === "forge_clone" && c.args.targetDir === "/home/me/code/tauri")).toBe(true));
    expect(await screen.findByText(/Cloned to \/home\/me\/code\/tauri/)).toBeTruthy();
  });

  it("does nothing if the folder pick is cancelled", async () => {
    openMock.mockResolvedValueOnce(null);
    const calls = route({});
    render(RepoBrowser, { props: { repo: "o/r" } });
    await fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    await waitFor(() => expect(openMock).toHaveBeenCalled());
    expect(calls.some((c) => c.cmd === "forge_clone")).toBe(false);
  });

  it("surfaces a backend error inline", async () => {
    route({ browse: () => new Error("Repo 'o/r' not found (or private — add a token).") });
    render(RepoBrowser, { props: { repo: "o/r" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    expect(await screen.findByText(/not found/i)).toBeTruthy();
  });

  it("loads a saved token on mount and remembers it (CPE-439)", async () => {
    route({ savedToken: "ghp_saved123", browse: () => root });
    render(RepoBrowser, { props: { provider: "github", repo: "o/r" } });
    // The saved token pre-fills and Remember is checked.
    const tokenInput = screen.getByPlaceholderText(/token/i) as HTMLInputElement;
    await waitFor(() => expect(tokenInput.value).toBe("ghp_saved123"));
    expect((screen.getByRole("checkbox") as HTMLInputElement).checked).toBe(true);
  });
});
