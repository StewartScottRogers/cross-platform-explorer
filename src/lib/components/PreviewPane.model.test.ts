/**
 * CPE-1334 — 3D-model geometry summary: surfacing the CPE-1333 `read_model_info` command in the
 * preview pane (the metadata-pane fallback for epic CPE-118). Independent of `provider.kind` — the
 * section is additive above whatever the underlying provider renders (STL/OBJ/GLB currently fall to
 * the last-resort "hex" provider, see `provider.ts`).
 */
import { describe, it, expect, vi } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import PreviewPane from "./PreviewPane.svelte";
import type { DirEntry } from "../types";

const { readModelInfoMock } = vi.hoisted(() => ({
  readModelInfoMock: vi.fn(),
}));

vi.mock("../bindings.gen", () => ({
  commands: {
    readModelInfo: readModelInfoMock,
    // Unused by these tests, but PreviewPane calls it for provider.kind === "text"; a benign stub
    // keeps that reactive branch from throwing if it's ever hit.
    codeIntel: vi.fn(async () => ({ outline: [], folds: [], indent: [], minimap: [] })),
    // STL/OBJ/GLB all fall through to the last-resort "hex" provider (see provider.ts) alongside the
    // 3D section — HexView's own read is unrelated to this feature, so stub it to a harmless empty page
    // rather than let it surface its own (unrelated) error text in these tests.
    readFileRange: vi.fn(async () => ({ status: "ok", data: [] })),
  },
}));

const entry = (over: Partial<DirEntry>): DirEntry => ({
  name: "x",
  path: "/x",
  is_dir: false,
  size: 1,
  modified: 0,
  extension: "",
  hidden: false,
  is_symlink: false,
  ...over,
});

const stlInfo = {
  format: "Stl" as const,
  triangle_count: 12,
  vertex_count: 8,
  bounding_box: [0, 0, 0, 2, 3, 4] as [number, number, number, number, number, number],
  ascii: true,
};

const objInfo = {
  format: "Obj" as const,
  triangle_count: 6, // a face count, not necessarily triangles — must be labelled "Faces"
  vertex_count: 8,
  bounding_box: [-1, -1, -1, 1, 1, 1] as [number, number, number, number, number, number],
  ascii: true,
};

describe("PreviewPane — 3D-model geometry section (CPE-1334)", () => {
  it("renders the 3D section with format/count/vertex/dimension stats for a parseable STL", async () => {
    readModelInfoMock.mockResolvedValueOnce({ status: "ok", data: stlInfo });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "part.stl", path: "/models/part.stl", extension: "stl" }),
    });

    await waitFor(() => expect(container.querySelector('[data-testid="model-info-section"]')).toBeTruthy());
    expect(readModelInfoMock).toHaveBeenCalledWith("/models/part.stl");

    const section = container.querySelector('[data-testid="model-info-section"]')!;
    expect(section.textContent).toContain("STL");
    expect(section.textContent).toContain("Triangles");
    expect(section.textContent).toContain("12");
    expect(section.textContent).toContain("8"); // vertex count
    expect(section.textContent).toContain("ASCII"); // ascii: true
    expect(section.textContent).toContain("2 × 3 × 4"); // bounding-box DIMENSIONS (max-min), not raw bounds
  });

  it("labels OBJ's count 'Faces' rather than 'Triangles' (it's a face count, not guaranteed triangles)", async () => {
    readModelInfoMock.mockResolvedValueOnce({ status: "ok", data: objInfo });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "cube.obj", path: "/models/cube.obj", extension: "obj" }),
    });

    await waitFor(() => expect(container.querySelector('[data-testid="model-info-section"]')).toBeTruthy());
    const section = container.querySelector('[data-testid="model-info-section"]')!;
    expect(section.textContent).toContain("OBJ");
    expect(section.textContent).toContain("Faces");
    expect(section.textContent).not.toContain("Triangles");
  });

  it("shows NO 3D section (never an error) when the file isn't a parseable model", async () => {
    readModelInfoMock.mockResolvedValueOnce({ status: "ok", data: null });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "scene.glb", path: "/models/scene.glb", extension: "glb" }),
    });

    await waitFor(() => expect(readModelInfoMock).toHaveBeenCalledWith("/models/scene.glb"));
    expect(container.querySelector('[data-testid="model-info-section"]')).toBeNull();
  });

  it("drops a stale response when the selection changes mid-flight (generation guard)", async () => {
    let resolveFirst!: (v: unknown) => void;
    readModelInfoMock.mockImplementationOnce(
      () => new Promise((resolve) => { resolveFirst = resolve; }),
    );
    readModelInfoMock.mockResolvedValueOnce({ status: "ok", data: objInfo });

    const { container, rerender } = render(PreviewPane, {
      entry: entry({ name: "slow.stl", path: "/models/slow.stl", extension: "stl" }),
    });
    await waitFor(() => expect(readModelInfoMock).toHaveBeenCalledWith("/models/slow.stl"));

    // Selection moves on to a second model before the first request resolves.
    await rerender({ entry: entry({ name: "cube.obj", path: "/models/cube.obj", extension: "obj" }) });
    await waitFor(() => expect(readModelInfoMock).toHaveBeenCalledWith("/models/cube.obj"));
    await waitFor(() => expect(container.querySelector('[data-testid="model-info-section"]')).toBeTruthy());

    // The superseded first request now resolves — it must NOT clobber the second file's stats.
    resolveFirst({ status: "ok", data: stlInfo });
    await new Promise((r) => setTimeout(r, 0));

    const section = container.querySelector('[data-testid="model-info-section"]')!;
    expect(section.textContent).toContain("OBJ"); // the second (current) selection's format
    expect(section.textContent).not.toContain("STL"); // the stale first response was dropped
  });
});
