/**
 * NewLinkDialog (CPE-1207, epic CPE-715). Owns its own `create_symlink`/`create_hard_link` call (see
 * the component's own header comment for why), so these tests mock the Tauri `invoke` boundary rather
 * than treating the dialog as pure props-in/events-out.
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import NewLinkDialog from "./NewLinkDialog.svelte";

// Windows-only "Junction" kind (CPE-1210) — same navigator.platform sniff/mock pattern as
// ShellIntegration.test.ts's Windows-only shell-integration row.
function setPlatform(value: string) {
  Object.defineProperty(window.navigator, "platform", { value, configurable: true });
}
afterEach(() => setPlatform(""));

// A Tauri command returning `Result<T, E>` resolves the JS promise with T on Ok and REJECTS it with E on
// Err (real Tauri IPC semantics) — `bindings.gen.ts`'s try/catch is what turns that rejection into the
// typed `{ status: "error", error }` the component sees, so the mock must throw, not resolve, on error.
let failWith: string | null = null;
const invoke = vi.fn(async (_cmd?: string, _args?: unknown) => {
  if (failWith !== null) throw new Error(failWith);
  return null;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

async function fillAndCreate(kind?: "symlink" | "hardlink") {
  render(NewLinkDialog, { targetDir: "/repo" });
  if (kind === "hardlink") {
    await fireEvent.change(screen.getByTestId("new-link-kind"), { target: { value: "hardlink" } });
  }
  await fireEvent.input(screen.getByTestId("new-link-target"), { target: { value: "/repo/CPE-1045-marker.txt" } });
  await fireEvent.input(screen.getByTestId("new-link-name"), { target: { value: "marker-link" } });
  await fireEvent.click(screen.getByTestId("new-link-create"));
}

describe("NewLinkDialog (CPE-1207)", () => {
  it("builds the right create_symlink call (target + name) and dispatches created on success", async () => {
    failWith = null;
    invoke.mockClear();
    const { component } = render(NewLinkDialog, { targetDir: "/repo" });
    const created = vi.fn();
    component.$on("created", (e: CustomEvent<string>) => created(e.detail));

    await fireEvent.input(screen.getByTestId("new-link-target"), { target: { value: "/repo/CPE-1045-marker.txt" } });
    await fireEvent.input(screen.getByTestId("new-link-name"), { target: { value: "marker-link" } });
    await fireEvent.click(screen.getByTestId("new-link-create"));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_symlink", {
      target: "/repo/CPE-1045-marker.txt",
      linkPath: "/repo/marker-link",
    }));
    await waitFor(() => expect(created).toHaveBeenCalledWith("/repo/marker-link"));
  });

  it("switching kind to Hardlink builds a create_hard_link call instead", async () => {
    failWith = null;
    invoke.mockClear();
    await fillAndCreate("hardlink");

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_hard_link", {
      target: "/repo/CPE-1045-marker.txt",
      linkPath: "/repo/marker-link",
    }));
    expect(invoke).not.toHaveBeenCalledWith("create_symlink", expect.anything());
  });

  it("blocks creation locally (no backend call) when the target or name is empty", async () => {
    invoke.mockClear();
    render(NewLinkDialog, { targetDir: "/repo" });
    await fireEvent.click(screen.getByTestId("new-link-create"));
    expect(invoke).not.toHaveBeenCalled();
    expect(screen.getByTestId("new-link-error").textContent).toContain("target");
  });

  it("surfaces the Windows Developer-Mode/elevation error via the dispatched `error` event (not swallowed)", async () => {
    const winError = "Access is denied. (os error 5) (Windows symbolic links require Developer Mode or elevation)";
    failWith = winError;
    invoke.mockClear();
    const { component } = render(NewLinkDialog, { targetDir: "/repo" });
    const errorSpy = vi.fn();
    component.$on("error", (e: CustomEvent<string>) => errorSpy(e.detail));

    await fireEvent.input(screen.getByTestId("new-link-target"), { target: { value: "/repo/CPE-1045-marker.txt" } });
    await fireEvent.input(screen.getByTestId("new-link-name"), { target: { value: "marker-link" } });
    await fireEvent.click(screen.getByTestId("new-link-create"));

    await waitFor(() => expect(errorSpy).toHaveBeenCalledTimes(1));
    expect(errorSpy.mock.calls[0][0]).toContain(winError);
    // The dialog itself stays open (never auto-closes on a backend failure) and shows the same text
    // inline, so the app-wide `showNotice` toast (App.svelte's `on:error` handler) isn't the only trace.
    expect(screen.getByTestId("new-link-error").textContent).toContain(winError);
    expect(screen.getByTestId("new-link-create")).toBeTruthy();
  });

  it("shows the Junction kind only on Windows", async () => {
    setPlatform("Win32");
    render(NewLinkDialog, { targetDir: "/repo" });
    const options = Array.from((screen.getByTestId("new-link-kind") as HTMLSelectElement).options).map((o) => o.value);
    expect(options).toContain("junction");
  });

  it("hides the Junction kind off Windows", async () => {
    setPlatform("MacIntel");
    render(NewLinkDialog, { targetDir: "/repo" });
    const options = Array.from((screen.getByTestId("new-link-kind") as HTMLSelectElement).options).map((o) => o.value);
    expect(options).not.toContain("junction");
  });

  it("switching kind to Junction (Windows) builds a create_junction call", async () => {
    setPlatform("Win32");
    failWith = null;
    invoke.mockClear();
    render(NewLinkDialog, { targetDir: "/repo" });
    await fireEvent.change(screen.getByTestId("new-link-kind"), { target: { value: "junction" } });
    await fireEvent.input(screen.getByTestId("new-link-target"), { target: { value: "C:\\some\\dir" } });
    await fireEvent.input(screen.getByTestId("new-link-name"), { target: { value: "junction-link" } });
    await fireEvent.click(screen.getByTestId("new-link-create"));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_junction", {
      target: "C:\\some\\dir",
      linkPath: "/repo/junction-link",
    }));
    expect(invoke).not.toHaveBeenCalledWith("create_symlink", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith("create_hard_link", expect.anything());
  });

  it("shows a directory hint for the Junction kind (Windows)", async () => {
    setPlatform("Win32");
    render(NewLinkDialog, { targetDir: "/repo" });
    await fireEvent.change(screen.getByTestId("new-link-kind"), { target: { value: "junction" } });
    expect(screen.getByTestId("new-link-junction-hint")).toBeTruthy();
  });
});
