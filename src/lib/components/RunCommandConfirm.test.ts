/**
 * CPE-1390 — jsdom render-spec for RunCommandConfirm, the safety gate before spawning EXTERNAL OS
 * PROCESSES (CPE-783, epic CPE-711). Zero prior coverage on a dialog that shells out, so this pins the
 * SHIPPED behavior: exact command review, Run disabled with nothing to run, the typed `runCommand` call
 * per line, exit-code/stdout/stderr rendering, `failed()` styling, and that Escape/backdrop-close are
 * blocked while a run is in flight (so a stray dismiss can't look like it cancelled a still-running
 * external process).
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import RunCommandConfirm from "./RunCommandConfirm.svelte";

const { runCommandMock } = vi.hoisted(() => ({
  runCommandMock: vi.fn(),
}));

vi.mock("../bindings.gen", () => ({
  commands: {
    runCommand: runCommandMock,
  },
}));

const ok = (over: Partial<{ stdout: string; stderr: string; code: number | null; truncated: boolean }> = {}) => ({
  status: "ok" as const,
  data: { stdout: "", stderr: "", code: 0, truncated: false, ...over },
});

describe("RunCommandConfirm (CPE-1390)", () => {
  it("renders the exact command lines and count, and enables Run when there are some", () => {
    const { container } = render(RunCommandConfirm, {
      title: "Build",
      commands: ["npm run build", "npm test"],
      cwd: "/repo",
    });

    expect(screen.getByText("npm run build")).toBeTruthy();
    expect(screen.getByText("npm test")).toBeTruthy();
    expect(container.querySelectorAll(".cmds li")).toHaveLength(2);
    const warnText = container.querySelector(".warn")!.textContent!.replace(/\s+/g, " ");
    expect(warnText).toContain("This runs 2 external commands");
    expect(warnText).toContain("on your machine in /repo");
    const runBtn = screen.getByText("Run").closest("button") as HTMLButtonElement;
    expect(runBtn.disabled).toBe(false);
  });

  it("disables Run and shows the empty notice when there are 0 commands", () => {
    const { container } = render(RunCommandConfirm, { title: "Nothing", commands: [] });

    expect(screen.getByText("Nothing to run for the current selection.")).toBeTruthy();
    const warnText = container.querySelector(".warn")!.textContent!.replace(/\s+/g, " ");
    expect(warnText).toContain("This runs 0 external commands");
    const runBtn = screen.getByText("Run").closest("button") as HTMLButtonElement;
    expect(runBtn.disabled).toBe(true);
  });

  it("Run calls commands.runCommand(cmd, cwd||null) once per line, in order, and renders exit/stdout/stderr", async () => {
    runCommandMock.mockResolvedValueOnce(ok({ stdout: "built ok", code: 0 }));
    runCommandMock.mockResolvedValueOnce(ok({ stdout: "", stderr: "1 failing", code: 1 }));

    render(RunCommandConfirm, { title: "Build", commands: ["npm run build", "npm test"], cwd: "/repo" });
    await fireEvent.click(screen.getByText("Run"));

    await waitFor(() => expect(screen.getByText("built ok")).toBeTruthy());
    expect(runCommandMock).toHaveBeenNthCalledWith(1, "npm run build", "/repo");
    expect(runCommandMock).toHaveBeenNthCalledWith(2, "npm test", "/repo");
    expect(runCommandMock).toHaveBeenCalledTimes(2);

    expect(screen.getByText("exit 0")).toBeTruthy();
    expect(screen.getByText("exit 1")).toBeTruthy();
    expect(screen.getByText("1 failing")).toBeTruthy();
  });

  it("passes null (not empty string) for cwd when cwd is unset", async () => {
    runCommandMock.mockResolvedValueOnce(ok());
    render(RunCommandConfirm, { title: "T", commands: ["echo hi"] }); // cwd defaults to ""
    await fireEvent.click(screen.getByText("Run"));

    await waitFor(() => expect(runCommandMock).toHaveBeenCalledWith("echo hi", null));
  });

  it("marks a non-zero exit as failed() (err styling) but a zero exit as not failed", async () => {
    runCommandMock.mockResolvedValueOnce(ok({ code: 0 }));
    runCommandMock.mockResolvedValueOnce(ok({ code: 2, stderr: "boom" }));

    const { container } = render(RunCommandConfirm, { title: "T", commands: ["a", "b"] });
    await fireEvent.click(screen.getByText("Run"));

    await waitFor(() => expect(container.querySelectorAll(".res")).toHaveLength(2));
    const [first, second] = Array.from(container.querySelectorAll(".res"));
    expect(first.classList.contains("err")).toBe(false);
    expect(second.classList.contains("err")).toBe(true);
  });

  it("a thrown/rejected runCommand renders the error text and is treated as failed()", async () => {
    runCommandMock.mockRejectedValueOnce(new Error("spawn ENOENT"));

    const { container } = render(RunCommandConfirm, { title: "T", commands: ["missingcmd"] });
    await fireEvent.click(screen.getByText("Run"));

    await waitFor(() => expect(screen.getByText(/spawn ENOENT/)).toBeTruthy());
    expect(screen.getByText(/✗ Error: spawn ENOENT/)).toBeTruthy();
    const res = container.querySelector(".res")!;
    expect(res.classList.contains("err")).toBe(true);
  });

  it("an error-status Result (unwrap throws) is also rendered as failed()", async () => {
    runCommandMock.mockResolvedValueOnce({ status: "error", error: "permission denied" });

    const { container } = render(RunCommandConfirm, { title: "T", commands: ["sudo thing"] });
    await fireEvent.click(screen.getByText("Run"));

    await waitFor(() => expect(screen.getByText(/permission denied/)).toBeTruthy());
    expect(container.querySelector(".res")!.classList.contains("err")).toBe(true);
  });

  it("shows a truncated notice when the backend flags output as truncated", async () => {
    runCommandMock.mockResolvedValueOnce(ok({ code: 0, truncated: true }));
    render(RunCommandConfirm, { title: "T", commands: ["a"] });
    await fireEvent.click(screen.getByText("Run"));

    await waitFor(() => expect(screen.getByText(/output truncated/)).toBeTruthy());
  });

  it("Escape and backdrop-click are BLOCKED while running, but work once idle", async () => {
    let resolveRun!: (v: unknown) => void;
    runCommandMock.mockImplementationOnce(() => new Promise((resolve) => { resolveRun = resolve; }));

    const { component, container } = render(RunCommandConfirm, { title: "T", commands: ["slow"] });
    const closed = vi.fn();
    component.$on("close", closed);

    await fireEvent.click(screen.getByText("Run"));
    // Still mid-flight: the Run button reads "Running…" and is disabled.
    expect(screen.getByText("Running…")).toBeTruthy();
    expect((screen.getByText("Running…").closest("button") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByText("Cancel").closest("button") as HTMLButtonElement).disabled).toBe(true);

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(closed).not.toHaveBeenCalled();

    await fireEvent.click(container.querySelector(".backdrop")!);
    expect(closed).not.toHaveBeenCalled();

    // Let the in-flight command resolve — running flips back to false.
    resolveRun(ok({ code: 0 }));
    await waitFor(() => expect(screen.queryByText("Running…")).toBeNull());

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(closed).toHaveBeenCalledTimes(1);
  });

  it("Cancel and the X button dispatch close when idle (not running)", async () => {
    const { component: c1 } = render(RunCommandConfirm, { title: "T", commands: ["a"] });
    const closed1 = vi.fn();
    c1.$on("close", closed1);
    await fireEvent.click(screen.getByText("Cancel"));
    expect(closed1).toHaveBeenCalledTimes(1);

    const { component: c2, container } = render(RunCommandConfirm, { title: "T", commands: ["a"] });
    const closed2 = vi.fn();
    c2.$on("close", closed2);
    await fireEvent.click(container.querySelector(".x")!);
    expect(closed2).toHaveBeenCalledTimes(1);
  });

  it("clicking the backdrop when idle dispatches close; clicking inside the dialog does not", async () => {
    const { component, container } = render(RunCommandConfirm, { title: "T", commands: ["a"] });
    const closed = vi.fn();
    component.$on("close", closed);

    await fireEvent.click(container.querySelector(".dialog")!);
    expect(closed).not.toHaveBeenCalled();

    await fireEvent.click(container.querySelector(".backdrop")!);
    expect(closed).toHaveBeenCalledTimes(1);
  });
});
