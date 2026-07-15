/**
 * RepoBrowser render test (CPE-434/435) — browse a forge repo's tree in-app via the host
 * `forge_browse` command. Verifies the fetch wiring, folder navigation, and error surfacing with a
 * mocked `invoke`.
 */
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import RepoBrowser from "./RepoBrowser.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const root = [
  { name: "src", path: "src", is_dir: true, size: 0 },
  { name: "README.md", path: "README.md", is_dir: false, size: 1024 },
];
const srcDir = [{ name: "lib.rs", path: "src/lib.rs", is_dir: false, size: 42 }];

beforeEach(() => invokeMock.mockReset());

describe("RepoBrowser", () => {
  it("browses a repo and calls forge_browse with owner/name + provider", async () => {
    invokeMock.mockResolvedValueOnce(root);
    render(RepoBrowser, { props: { provider: "github", repo: "tauri-apps/tauri" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("forge_browse", {
      provider: "github", repo: "tauri-apps/tauri", path: "", token: null,
    }));
    expect(await screen.findByText("src")).toBeTruthy();
    expect(screen.getByText("README.md")).toBeTruthy();
  });

  it("navigates into a folder and back up", async () => {
    invokeMock.mockResolvedValueOnce(root).mockResolvedValueOnce(srcDir);
    render(RepoBrowser, { props: { repo: "o/r" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    await screen.findByText("src");

    await fireEvent.click(screen.getByText("src")); // into src/
    await waitFor(() => expect(invokeMock).toHaveBeenLastCalledWith("forge_browse", expect.objectContaining({ path: "src" })));
    expect(await screen.findByText("lib.rs")).toBeTruthy();
    expect(screen.getByText("..")).toBeTruthy(); // up affordance appears in a subfolder
  });

  it("rejects a bare name without owner/, without hitting the backend", async () => {
    render(RepoBrowser, { props: { repo: "justaname" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    expect(invokeMock).not.toHaveBeenCalled();
    expect(await screen.findByText(/owner\/name/i)).toBeTruthy();
  });

  it("surfaces a backend error inline", async () => {
    invokeMock.mockRejectedValueOnce(new Error("Repo 'o/r' not found (or private — add a token)."));
    render(RepoBrowser, { props: { repo: "o/r" } });
    await fireEvent.click(screen.getByRole("button", { name: "Browse" }));
    expect(await screen.findByText(/not found/i)).toBeTruthy();
  });
});
