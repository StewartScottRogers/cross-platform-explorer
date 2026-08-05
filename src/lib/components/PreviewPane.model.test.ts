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
  mesh_count: 0,
};

const objInfo = {
  format: "Obj" as const,
  triangle_count: 6, // a face count, not necessarily triangles — must be labelled "Faces"
  vertex_count: 8,
  bounding_box: [-1, -1, -1, 1, 1, 1] as [number, number, number, number, number, number],
  ascii: true,
  mesh_count: 0,
};

// CPE-1336: glTF/GLB never has a triangle/vertex count (see ModelInfo doc comment) — it gets its own
// "Meshes" row instead, and the triangle/face + vertex rows must be suppressed rather than showing a
// misleading "0 Triangles" / "0 Vertices".
const gltfInfo = {
  format: "Gltf" as const,
  triangle_count: 0,
  vertex_count: 0,
  bounding_box: [-2, -1, -1, 2, 1, 1] as [number, number, number, number, number, number],
  ascii: true,
  mesh_count: 3,
};

// A glTF with no POSITION accessor min/max carries an all-zero bounding box — the dimensions row
// must be omitted entirely rather than showing "0 × 0 × 0".
const gltfInfoZeroBbox = {
  format: "Gltf" as const,
  triangle_count: 0,
  vertex_count: 0,
  bounding_box: [0, 0, 0, 0, 0, 0] as [number, number, number, number, number, number],
  ascii: false,
  mesh_count: 2,
};

// CPE-1337 (backend) / CPE-1340 (frontend): PLY's element-face count is a FACE count too (not
// guaranteed triangles), same caveat as OBJ — must be labelled "Faces", not "Triangles".
const plyInfo = {
  format: "Ply" as const,
  triangle_count: 20, // "element face" count — a face count, not necessarily triangles
  vertex_count: 12,
  bounding_box: [-1, -2, -3, 1, 2, 3] as [number, number, number, number, number, number],
  ascii: true,
  mesh_count: 0,
};

// Binary PLY carries a zeroed bounding box (not computed for the binary flavour) — the dimensions
// row must be omitted, same as the glTF no-extrema case.
const plyInfoZeroBbox = {
  format: "Ply" as const,
  triangle_count: 20,
  vertex_count: 12,
  bounding_box: [0, 0, 0, 0, 0, 0] as [number, number, number, number, number, number],
  ascii: false,
  mesh_count: 0,
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

  // CPE-1336: CPE-1335 added the Gltf format + mesh_count, but the pane didn't handle either — a
  // glTF/GLB file rendered a blank format row and a misleading "0 Triangles".
  it("renders glTF as 'glTF' with a Meshes count + dimensions, and suppresses the triangle/vertex rows", async () => {
    readModelInfoMock.mockResolvedValueOnce({ status: "ok", data: gltfInfo });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "scene.gltf", path: "/models/scene.gltf", extension: "gltf" }),
    });

    await waitFor(() => expect(container.querySelector('[data-testid="model-info-section"]')).toBeTruthy());
    const section = container.querySelector('[data-testid="model-info-section"]')!;
    expect(section.textContent).toContain("glTF");
    expect(section.textContent).toContain("Meshes");
    expect(section.textContent).toContain("3"); // mesh_count
    expect(section.textContent).toContain("4 × 2 × 2"); // bounding-box DIMENSIONS (max-min)
    expect(section.textContent).not.toContain("Triangles");
    expect(section.textContent).not.toContain("Vertices");
    expect(section.textContent).not.toContain("0 Triangles");
    expect(section.textContent).not.toContain("0 Vertices");
  });

  it("omits the Dimensions row for a glTF with an all-zero bounding box (no POSITION accessor extrema)", async () => {
    readModelInfoMock.mockResolvedValueOnce({ status: "ok", data: gltfInfoZeroBbox });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "scene.glb", path: "/models/scene.glb", extension: "glb" }),
    });

    await waitFor(() => expect(container.querySelector('[data-testid="model-info-section"]')).toBeTruthy());
    const section = container.querySelector('[data-testid="model-info-section"]')!;
    expect(section.textContent).toContain("glTF");
    expect(section.textContent).toContain("Meshes");
    expect(section.textContent).toContain("2"); // mesh_count
    expect(section.textContent).not.toContain("Dimensions");
    expect(section.textContent).not.toContain("0 × 0 × 0");
  });

  // CPE-1340: CPE-1337 (backend) added the Ply format, but the pane didn't handle it — a .ply file
  // triggered no readModelInfo call (missing from MODEL_EXTS), and modelFormatLabel had no Ply arm.
  it("renders PLY as 'PLY' with a Faces count + dimensions, like OBJ (not 'Triangles')", async () => {
    readModelInfoMock.mockResolvedValueOnce({ status: "ok", data: plyInfo });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "scan.ply", path: "/models/scan.ply", extension: "ply" }),
    });

    await waitFor(() => expect(container.querySelector('[data-testid="model-info-section"]')).toBeTruthy());
    expect(readModelInfoMock).toHaveBeenCalledWith("/models/scan.ply");

    const section = container.querySelector('[data-testid="model-info-section"]')!;
    expect(section.textContent).toContain("PLY");
    expect(section.textContent).toContain("Faces");
    expect(section.textContent).toContain("20"); // face count
    expect(section.textContent).toContain("12"); // vertex count
    expect(section.textContent).toContain("2 × 4 × 6"); // bounding-box DIMENSIONS (max-min)
    expect(section.textContent).not.toContain("Triangles");
  });

  it("omits the Dimensions row for a binary PLY with an all-zero bounding box", async () => {
    readModelInfoMock.mockResolvedValueOnce({ status: "ok", data: plyInfoZeroBbox });

    const { container } = render(PreviewPane, {
      entry: entry({ name: "scan.ply", path: "/models/scan.ply", extension: "ply" }),
    });

    await waitFor(() => expect(container.querySelector('[data-testid="model-info-section"]')).toBeTruthy());
    const section = container.querySelector('[data-testid="model-info-section"]')!;
    expect(section.textContent).toContain("PLY");
    expect(section.textContent).toContain("Faces");
    expect(section.textContent).not.toContain("Dimensions");
    expect(section.textContent).not.toContain("0 × 0 × 0");
  });
});
