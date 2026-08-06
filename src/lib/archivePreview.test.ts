import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  extractInnerToTemp,
  resolveArchivePreviewEntry,
  createArchivePreviewResolver,
} from "./archivePreview";
import type { DirEntry } from "./types";

// The typed `commands.*` client routes every call through `./invoke`'s `invoke`, and archivePreview also
// pulls `unwrap` from there — so mocking `./invoke` lets the test stand in for the whole extract backend.
const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => "");
vi.mock("./invoke", () => ({
  invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a),
  unwrap: <T>(r: { status: string; data?: T; error?: unknown }): T => {
    if (r.status === "ok") return r.data as T;
    throw r.error instanceof Error ? r.error : new Error(String(r.error));
  },
}));

function file(name: string, path: string): DirEntry {
  return {
    name,
    path,
    is_dir: false,
    size: 10,
    modified: null,
    extension: name.includes(".") ? name.split(".").pop()!.toLowerCase() : "",
    hidden: false,
    is_symlink: false,
  };
}

function dir(name: string, path: string): DirEntry {
  return { ...file(name, path), is_dir: true, extension: "" };
}

describe("archivePreview extract-then-preview (CPE-1360)", () => {
  beforeEach(() => invokeMock.mockReset());

  it("routes a ZIP-family archive to extract_archive_entry", async () => {
    invokeMock.mockResolvedValue("C:/temp/cpe-archive/todo.txt");
    const temp = await extractInnerToTemp("C:/x/a.zip", "notes/todo.txt");
    expect(temp).toBe("C:/temp/cpe-archive/todo.txt");
    expect(invokeMock).toHaveBeenCalledWith("extract_archive_entry", { zip: "C:/x/a.zip", inner: "notes/todo.txt" });
  });

  it("routes a .rar to extract_rar_entry (Part B command)", async () => {
    invokeMock.mockResolvedValue("C:/temp/cpe-archive/todo.txt");
    await extractInnerToTemp("C:/x/a.rar", "notes/todo.txt");
    expect(invokeMock).toHaveBeenCalledWith("extract_rar_entry", { path: "C:/x/a.rar", inner: "notes/todo.txt" });
  });

  it("routes tar/7z to the format-agnostic extract_archive_entry_any", async () => {
    invokeMock.mockResolvedValue("C:/temp/cpe-archive/x");
    await extractInnerToTemp("C:/x/a.tar.gz", "d/x");
    expect(invokeMock).toHaveBeenCalledWith("extract_archive_entry_any", { path: "C:/x/a.tar.gz", inner: "d/x" });
    invokeMock.mockClear();
    await extractInnerToTemp("C:/x/a.7z", "d/y");
    expect(invokeMock).toHaveBeenCalledWith("extract_archive_entry_any", { path: "C:/x/a.7z", inner: "d/y" });
  });

  it("caches per (zipPath, inner) so re-selecting doesn't re-extract (temp still on disk)", async () => {
    // The mock resolves every command — including the `entry_info` re-validation stat — to a truthy value,
    // so the cached temp reads as still-present and the second select is served from cache.
    invokeMock.mockResolvedValue("C:/temp/cpe-archive/cached.txt");
    const first = await extractInnerToTemp("C:/x/cache.zip", "cached.txt");
    const second = await extractInnerToTemp("C:/x/cache.zip", "cached.txt");
    expect(first).toBe(second);
    const extractCalls = invokeMock.mock.calls.filter((c) => c[0] === "extract_archive_entry");
    expect(extractCalls).toHaveLength(1); // extraction happened once; the 2nd select re-used the cache
  });

  it("re-extracts when the cached temp file was reaped mid-session (entry_info reports it gone)", async () => {
    // entry_info (the cache re-validation stat) throws → the cached path is dead → the second select must
    // drop the stale entry and re-extract to the stable temp path, not hand a loader a dead path.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "entry_info") throw new Error("no such file"); // temp reaped by the OS
      return "C:/temp/cpe-archive/reaped.txt";
    });
    const a = await extractInnerToTemp("C:/x/reap.zip", "reaped.txt");
    const b = await extractInnerToTemp("C:/x/reap.zip", "reaped.txt");
    expect(a).toBe(b);
    const extractCalls = invokeMock.mock.calls.filter((c) => c[0] === "extract_archive_entry");
    expect(extractCalls).toHaveLength(2); // cache hit was invalidated → extracted again
  });

  it("resolveArchivePreviewEntry keeps name/extension but points path at the temp file", async () => {
    invokeMock.mockResolvedValue("C:/temp/cpe-archive/photo.png");
    const resolved = await resolveArchivePreviewEntry("C:/x/pics.zip", file("photo.png", "album/photo.png"));
    expect(resolved.path).toBe("C:/temp/cpe-archive/photo.png"); // real temp path for the loaders
    expect(resolved.name).toBe("photo.png"); // provider selection unchanged
    expect(resolved.extension).toBe("png");
  });
});

describe("createArchivePreviewResolver request-id guard (CPE-1360)", () => {
  beforeEach(() => invokeMock.mockReset());

  it("does not preview a directory selection", () => {
    const onResolved = vi.fn();
    const r = createArchivePreviewResolver(onResolved);
    r.update({ zipPath: "C:/x/a.zip" }, [dir("sub", "sub")]);
    expect(onResolved).toHaveBeenLastCalledWith(null);
    expect(invokeMock).not.toHaveBeenCalled(); // no extraction attempted for a folder
  });

  it("clears the preview when the archive is exited or selection isn't a single file", () => {
    const onResolved = vi.fn();
    const r = createArchivePreviewResolver(onResolved);
    r.update(null, [file("a.txt", "a.txt")]);
    expect(onResolved).toHaveBeenLastCalledWith(null);
    r.update({ zipPath: "C:/x/a.zip" }, [file("a.txt", "a.txt"), file("b.txt", "b.txt")]);
    expect(onResolved).toHaveBeenLastCalledWith(null);
  });

  it("drops a stale extraction when the selection changes mid-flight", async () => {
    // Two overlapping extractions; the FIRST resolves LAST. The guard must ignore the stale first result
    // and keep only the newest selection's entry.
    let resolveFirst!: (v: string) => void;
    let resolveSecond!: (v: string) => void;
    invokeMock
      .mockImplementationOnce(() => new Promise<string>((res) => { resolveFirst = res; }))
      .mockImplementationOnce(() => new Promise<string>((res) => { resolveSecond = res; }));

    const onResolved = vi.fn();
    const r = createArchivePreviewResolver(onResolved);
    const arch = { zipPath: "C:/x/a.zip" };
    r.update(arch, [file("old.txt", "old.txt")]); // request 1 (stale)
    r.update(arch, [file("new.txt", "new.txt")]); // request 2 (current)

    const flush = () => new Promise((r) => setTimeout(r, 0));
    resolveSecond("C:/temp/cpe-archive/new.txt"); // newest lands first
    await flush();
    resolveFirst("C:/temp/cpe-archive/old.txt"); // stale lands late — must be ignored
    await flush();

    const applied = onResolved.mock.calls.map((c) => c[0]).filter((e) => e !== null) as DirEntry[];
    expect(applied).toHaveLength(1);
    expect(applied[0].name).toBe("new.txt");
    expect(applied[0].path).toBe("C:/temp/cpe-archive/new.txt");
  });
});
