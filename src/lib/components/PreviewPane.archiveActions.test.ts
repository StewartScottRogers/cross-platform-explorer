/**
 * CPE-1578 (epic CPE-1568 slice 4) — the `archive` provider's declared `actions` (`preview/provider.ts`):
 * Extract / Extract to… / Check archive safety… on the preview action bar (CPE-1570), pure UI wiring
 * onto the app's EXISTING context-menu backend paths (queued transfer extract + `analyze_archive_safety`
 * scan) — this test only asserts the pane calls the app-supplied callbacks with the previewed entry, not
 * the backend itself (that's covered by the existing App.svelte archive tests). Mirrors
 * `PreviewPane.imageActions.test.ts`/`jsonActions`'s recipe.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/svelte";
import PreviewPane from "./PreviewPane.svelte";
import type { DirEntry } from "../types";

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

function actionBarLabels(container: Element): (string | undefined)[] {
  const bar = container.querySelector('[data-testid="preview-action-bar"]');
  if (!bar) return [];
  return [...bar.querySelectorAll("button")].map((b) => b.textContent?.trim());
}

describe("PreviewPane — archive provider action bar (CPE-1578)", () => {
  it("renders Extract / Extract to… / Check archive safety… for a zip-family archive", async () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.zip", path: "/d/a.zip", extension: "zip" }),
      loadEntries: async () => [],
    });
    await waitFor(() => expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeTruthy());
    expect(actionBarLabels(container)).toEqual(["Extract", "Extract to…", "Check archive safety…"]);
  });

  it("gates Check archive safety off for a non-ZIP extractable archive (tar), keeps Extract/Extract to…", async () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.tar", path: "/d/a.tar", extension: "tar" }),
      loadEntries: async () => [],
    });
    await waitFor(() => expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeTruthy());
    expect(actionBarLabels(container)).toEqual(["Extract", "Extract to…"]);
  });

  it("shows no archive actions for a browse-only format (iso): not extractable, not safety-scorable", async () => {
    const { container } = render(PreviewPane, {
      entry: entry({ name: "a.iso", path: "/d/a.iso", extension: "iso" }),
      loadEntries: async () => [],
    });
    // The action bar itself is only mounted when there's at least one visible action.
    await waitFor(() => expect(screen.queryByText(/item/i)).toBeTruthy()); // wait for the entries note to settle
    expect(container.querySelector('[data-testid="preview-action-bar"]')).toBeFalsy();
  });

  it("Extract runs the app-supplied extractArchiveHere with the previewed entry", async () => {
    const extractArchiveHere = vi.fn().mockResolvedValue(undefined);
    render(PreviewPane, {
      entry: entry({ name: "a.zip", path: "/d/a.zip", extension: "zip" }),
      loadEntries: async () => [],
      extractArchiveHere,
    });

    const btn = await waitFor(() => screen.getByRole("button", { name: /^Extract$/ }));
    await fireEvent.click(btn);

    expect(extractArchiveHere).toHaveBeenCalledWith(
      expect.objectContaining({ path: "/d/a.zip", name: "a.zip" }),
    );
  });

  it("Extract to… runs the app-supplied extractArchiveTo with the previewed entry", async () => {
    const extractArchiveTo = vi.fn().mockResolvedValue(undefined);
    render(PreviewPane, {
      entry: entry({ name: "a.zip", path: "/d/a.zip", extension: "zip" }),
      loadEntries: async () => [],
      extractArchiveTo,
    });

    const btn = await waitFor(() => screen.getByRole("button", { name: /Extract to…/ }));
    await fireEvent.click(btn);

    expect(extractArchiveTo).toHaveBeenCalledWith(
      expect.objectContaining({ path: "/d/a.zip", name: "a.zip" }),
    );
  });

  it("Check archive safety… runs the app-supplied checkArchiveSafety with the previewed entry", async () => {
    const checkArchiveSafety = vi.fn();
    render(PreviewPane, {
      entry: entry({ name: "a.zip", path: "/d/a.zip", extension: "zip" }),
      loadEntries: async () => [],
      checkArchiveSafety,
    });

    const btn = await waitFor(() => screen.getByRole("button", { name: /Check archive safety…/ }));
    await fireEvent.click(btn);

    expect(checkArchiveSafety).toHaveBeenCalledWith(
      expect.objectContaining({ path: "/d/a.zip", name: "a.zip" }),
    );
  });

  it("without app wiring (defaults), clicking the actions is a harmless no-op", async () => {
    render(PreviewPane, {
      entry: entry({ name: "a.zip", path: "/d/a.zip", extension: "zip" }),
      loadEntries: async () => [],
    });
    const btn = await waitFor(() => screen.getByRole("button", { name: /^Extract$/ }));
    await expect(fireEvent.click(btn)).resolves.toBeTruthy();
  });
});
