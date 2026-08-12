/**
 * RepoBrowser render test (CPE-434/435/436/439) — browse a forge repo's tree in-app, clone it, and
 * remember the token. Uses a command-router `invoke` mock (the component also calls forge_get_token
 * on mount and forge_set/delete_token on Remember, so a sequential mock would mis-align).
 */
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
import RepoBrowser, { stripRepoUrl, looksLikeUrl } from "./RepoBrowser.svelte";

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

// CPE-1620 — the URL-stripping regex was hardcoded to github.com, so pasting a GitLab/Bitbucket/
// Codeberg URL while that provider was selected fell through to the backend as a fake "owner/name"
// instead of getting the friendly "enter owner/name" guidance.
describe("stripRepoUrl / looksLikeUrl (CPE-1620)", () => {
  it.each([
    ["github", "https://github.com/owner/name"],
    ["gitlab", "https://gitlab.com/owner/name"],
    ["bitbucket", "https://bitbucket.org/owner/name"],
    ["codeberg", "https://codeberg.org/owner/name"],
  ])("strips the %s host from its own URL", (provider, url) => {
    expect(stripRepoUrl(url, provider)).toBe("owner/name");
    expect(looksLikeUrl(stripRepoUrl(url, provider))).toBe(false);
  });

  it("strips a trailing .git after the host strip", () => {
    expect(stripRepoUrl("https://gitlab.com/owner/name.git", "gitlab")).toBe("owner/name");
  });

  it("leaves owner/name untouched for every provider", () => {
    for (const provider of ["github", "gitlab", "bitbucket", "codeberg", "generic"]) {
      expect(stripRepoUrl("owner/name", provider)).toBe("owner/name");
    }
  });

  it("negative control: a foreign-host URL is not stripped and still looks like a URL", () => {
    // Bitbucket selected, but a GitLab URL pasted — the strip must not fire for the wrong host.
    const r = stripRepoUrl("https://gitlab.com/owner/name", "bitbucket");
    expect(r).toBe("https://gitlab.com/owner/name");
    expect(looksLikeUrl(r)).toBe(true);
  });
});

describe("RepoBrowser — per-provider URL paste (CPE-1620)", () => {
  it.each([
    ["gitlab", "https://gitlab.com/owner/name"],
    ["bitbucket", "https://bitbucket.org/owner/name"],
    ["codeberg", "https://codeberg.org/owner/name"],
  ])("browses correctly after pasting a %s URL", async (provider, url) => {
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider, repo: url } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));

    await waitFor(() =>
      expect(calls.some((c) => c.cmd === "forge_browse" && c.args.repo === "owner/name")).toBe(true),
    );
    expect(await screen.findByText("src")).toBeTruthy();
  });

  it("shows the friendly owner/name guidance for a foreign-host URL instead of forwarding it", async () => {
    // GitLab selected, but a GitHub URL pasted — must not reach forge_browse with the raw URL.
    const calls = route({ browse: () => root });
    render(RepoBrowser, { props: { provider: "gitlab", repo: "https://github.com/owner/name" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));

    expect(calls.some((c) => c.cmd === "forge_browse")).toBe(false);
    expect(await screen.findByText(/owner\/name/i)).toBeTruthy();
  });
});
