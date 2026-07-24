/**
 * TemplatesDialog (CPE-837, epic CPE-740). The dialog is a thin render over the typed folder-template
 * commands; these assert it lists templates, captures the current folder (capture → save), and stamps a
 * selected template into the current folder (load → stamp) emitting a `stamped` event. The typed
 * `commands.*` client routes through the mocked `../invoke`, so mocking `invoke` here drives it.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";

const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null);
vi.mock("../invoke", () => ({
  invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a),
  unwrap: <T>(r: { status: string; data?: T; error?: unknown }): T => {
    if (r.status === "ok") return r.data as T;
    throw r.error instanceof Error ? r.error : new Error(String(r.error));
  },
}));

import TemplatesDialog from "./TemplatesDialog.svelte";
import { buildVars } from "../templateVars";

const SUMMARIES = [
  { name: "rust-crate", dirs: 3, files: 2 },
  { name: "blog-post", dirs: 1, files: 1 },
];

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => (cmd === "template_list" ? SUMMARIES : null));
});

describe("TemplatesDialog buildVars (CPE-837)", () => {
  it("always supplies {date} as today's ISO date", () => {
    const vars = buildVars("");
    expect(vars.date).toBe(new Date().toISOString().slice(0, 10));
    expect("name" in vars).toBe(false); // no {name} when the input is blank
  });

  it("adds a trimmed {name} when provided", () => {
    expect(buildVars("  My Project  ").name).toBe("My Project");
  });
});

describe("TemplatesDialog (CPE-837)", () => {
  it("lists stored templates on open", async () => {
    render(TemplatesDialog, { path: "/work/proj" });
    expect(await screen.findByTestId("tpl-rust-crate")).toBeTruthy();
    expect(screen.getByTestId("tpl-blog-post")).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith("template_list");
  });

  it("captures the current folder (capture → save)", async () => {
    render(TemplatesDialog, { path: "/work/proj" });
    await screen.findByTestId("tpl-rust-crate");
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "template_capture") return { name: "snap", nodes: [] };
      if (cmd === "template_save") return {};
      if (cmd === "template_list") return SUMMARIES;
      return null;
    });

    await fireEvent.input(screen.getByLabelText("New template name"), { target: { value: "snap" } });
    await fireEvent.click(screen.getByTestId("capture-btn"));

    expect(invokeMock).toHaveBeenCalledWith("template_capture", { path: "/work/proj", name: "snap" });
    expect(invokeMock).toHaveBeenCalledWith("template_save", { template: { name: "snap", nodes: [] } });
  });

  it("stamps the selected template into the current folder and emits stamped", async () => {
    const { component } = render(TemplatesDialog, { path: "/work/proj" });
    await screen.findByTestId("tpl-rust-crate");

    const stamped = vi.fn();
    component.$on("stamped", (e: CustomEvent) => stamped(e.detail));

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "template_load") return { name: "rust-crate", nodes: [] };
      if (cmd === "template_stamp") return ["/work/proj/src", "/work/proj/Cargo.toml"];
      if (cmd === "template_list") return SUMMARIES;
      return null;
    });

    await fireEvent.click(screen.getByTestId("tpl-rust-crate")); // select
    await fireEvent.click(screen.getByTestId("stamp-btn"));

    expect(invokeMock).toHaveBeenCalledWith("template_load", { name: "rust-crate" });
    const stampCall = invokeMock.mock.calls.find((c) => c[0] === "template_stamp");
    expect(stampCall?.[1]).toMatchObject({ dest: "/work/proj" });
    expect(stamped).toHaveBeenCalledWith({ dest: "/work/proj", count: 2 });
  });
});
