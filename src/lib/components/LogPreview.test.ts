import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import LogPreview from "./LogPreview.svelte";
import { MAX_LINES } from "../preview/logViewer";

// CPE-1618 (epic CPE-1568 slice 8): jsdom render-spec for the log preview, wiring the generic
// `read_file_text` backend command into a standalone component (same mocking recipe as
// NotebookPreview.test.ts/CertPreview.test.ts: mock `../bindings.gen`'s `commands` object). jsdom can't
// see layout, so these assert text/DOM content, filter behaviour, and robustness paths only — never a
// visual verdict (see the dedicated structural CSS guard at the bottom for the bounded-scroll claim).

const { readFileTextMock } = vi.hoisted(() => ({ readFileTextMock: vi.fn() }));

vi.mock("../bindings.gen", () => ({
  commands: { readFileText: readFileTextMock },
}));

function ok(text: string) {
  return { status: "ok" as const, data: text };
}

beforeEach(() => {
  readFileTextMock.mockReset();
});

const MIXED_LOG = [
  "[2026-08-11 09:14:01] INFO  Starting service",
  "[2026-08-11 09:14:02] WARN  Config value missing, using default",
  "[2026-08-11 09:14:03] ERROR Failed to connect to database",
  "[2026-08-11 09:14:03] DEBUG Retrying connection",
  "Request payload: userId=42 action=checkout",
].join("\n");

describe("LogPreview (CPE-1618)", () => {
  it("renders one row per line, each tinted by its detected level", async () => {
    readFileTextMock.mockResolvedValueOnce(ok(MIXED_LOG));

    const { container } = render(LogPreview, { path: "/x/app.log" });

    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(5));
    expect(readFileTextMock).toHaveBeenCalledWith("/x/app.log", expect.any(Number));

    const rows = Array.from(container.querySelectorAll('[data-testid="log-row"]'));
    expect(rows.map((r) => r.getAttribute("data-level"))).toEqual(["info", "warn", "error", "debug", "none"]);
    expect(container.textContent).toContain("Failed to connect to database");
  });

  it("shows per-level counts in the filter chips", async () => {
    readFileTextMock.mockResolvedValueOnce(ok(MIXED_LOG));

    const { container } = render(LogPreview, { path: "/x/app.log" });

    await waitFor(() => expect(container.querySelector('[data-testid="log-filter-chip-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="log-filter-chip-error"]')!.textContent).toContain("1");
    expect(container.querySelector('[data-testid="log-filter-chip-info"]')!.textContent).toContain("1");
  });

  it("filter chips actually hide non-matching rows and the visible count updates", async () => {
    readFileTextMock.mockResolvedValueOnce(ok(MIXED_LOG));

    const { container } = render(LogPreview, { path: "/x/app.log" });
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(5));

    // Turn off everything except error: click every other active chip to deactivate it.
    for (const level of ["info", "warn", "debug"]) {
      await fireEvent.click(container.querySelector(`[data-testid="log-filter-chip-${level}"]`)!);
    }
    await fireEvent.click(container.querySelector('[data-testid="log-filter-chip-unleveled"]')!);

    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(1));
    expect(container.querySelector('[data-testid="log-row"]')!.getAttribute("data-level")).toBe("error");
    expect(container.querySelector('[data-testid="log-visible-count"]')!.textContent).toMatch(/1 of 5/);
  });

  it("re-activating a chip shows its rows again", async () => {
    readFileTextMock.mockResolvedValueOnce(ok(MIXED_LOG));

    const { container } = render(LogPreview, { path: "/x/app.log" });
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(5));

    const errorChip = container.querySelector('[data-testid="log-filter-chip-error"]')!;
    await fireEvent.click(errorChip); // off
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(4));
    await fireEvent.click(errorChip); // back on
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(5));
  });

  it("does not misclassify a line that merely mentions 'error' in prose (end-to-end through the component)", async () => {
    readFileTextMock.mockResolvedValueOnce(
      ok("User asked about a checkout error they saw yesterday.\nINFO normal line"),
    );

    const { container } = render(LogPreview, { path: "/x/app.log" });
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(2));

    const rows = Array.from(container.querySelectorAll('[data-testid="log-row"]'));
    expect(rows[0].getAttribute("data-level")).toBe("none");
    expect(rows[1].getAttribute("data-level")).toBe("info");
  });

  it("strips ANSI escape codes end-to-end so no literal escape-code garbage reaches the DOM", async () => {
    const esc = String.fromCharCode(27);
    readFileTextMock.mockResolvedValueOnce(ok(esc + "[31mERROR" + esc + "[0m payment failed"));

    const { container } = render(LogPreview, { path: "/x/app.log" });
    await waitFor(() => expect(container.querySelector('[data-testid="log-row"]')).toBeTruthy());

    const row = container.querySelector('[data-testid="log-row"]')!;
    expect(row.textContent).not.toContain("[31m");
    expect(row.textContent).not.toContain("[0m");
    expect(row.textContent).toContain("ERROR payment failed");
  });

  it("shows an honest 'showing N of M' note for a file with far more lines than MAX_LINES, and stays capped", async () => {
    const totalLines = MAX_LINES + 250;
    const text = Array.from({ length: totalLines }, (_, i) => `INFO line ${i}`).join("\n");
    readFileTextMock.mockResolvedValueOnce(ok(text));

    const { container } = render(LogPreview, { path: "/x/huge.log" });

    await waitFor(() => expect(container.querySelector('[data-testid="log-lines-capped"]')).toBeTruthy());
    expect(container.querySelectorAll('[data-testid="log-row"]').length).toBeLessThanOrEqual(MAX_LINES);
    expect(container.querySelector('[data-testid="log-lines-capped"]')!.textContent).toMatch(
      new RegExp(`of ${totalLines.toLocaleString()}`),
    );
  });

  it("shows a load-error state when the invoke call itself fails, without crashing", async () => {
    readFileTextMock.mockRejectedValueOnce(new Error("File is too large to preview"));

    const { container } = render(LogPreview, { path: "/x/huge.log" });

    await waitFor(() => expect(container.querySelector('[data-testid="log-load-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="log-load-error"]')!.textContent).toContain("too large");
  });

  it("handles an empty file without crashing, rendering a clear 'empty' note rather than nothing at all", async () => {
    readFileTextMock.mockResolvedValueOnce(ok(""));

    const { container } = render(LogPreview, { path: "/x/empty.log" });

    await waitFor(() => expect(container.textContent).toMatch(/empty/i));
    expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(0);
  });

  it("switching to a new path re-loads and replaces the previous file's rows", async () => {
    readFileTextMock.mockResolvedValueOnce(ok("ERROR first file"));
    const { container, rerender } = render(LogPreview, { path: "/x/a.log" });
    await waitFor(() => expect(container.textContent).toContain("first file"));

    readFileTextMock.mockResolvedValueOnce(ok("INFO second file"));
    await rerender({ path: "/x/b.log" });
    await waitFor(() => expect(container.textContent).toContain("second file"));
    expect(container.textContent).not.toContain("first file");
  });
});

// Structural guard (not a visual one — jsdom can't compute a real max-height/scrollbar): asserts the CSS
// actually bounds `.log-body` to its own scroll region within a height-constrained ancestor, matching the
// `.hexview`/`.preview` convention this app already uses for other self-contained, full-pane previews —
// a human/Visual-Critic screenshot pass is what confirms it looks right on screen.
describe("LogPreview bounded scroll region (structural CSS guard)", () => {
  const src = readFileSync(join(__dirname, "LogPreview.svelte"), "utf8");
  const styleMatch = src.match(/<style>([\s\S]*)<\/style>/);
  const css = styleMatch![1];

  it("has a <style> block", () => {
    expect(styleMatch, "LogPreview.svelte must have a <style> block").toBeTruthy();
  });

  it(".log-preview fills its container's height so .log-body's flex-fill has something real to bound against", () => {
    const ruleMatch = css.match(/\.log-preview\s*\{([^}]*)\}/);
    expect(ruleMatch, ".log-preview rule not found").toBeTruthy();
    expect(ruleMatch![1]).toMatch(/height\s*:\s*100%/);
  });

  it(".log-body is its own scroll region, not silently clipped or left to grow the whole pane unbounded", () => {
    const ruleMatch = css.match(/\.log-body\s*\{([^}]*)\}/);
    expect(ruleMatch, ".log-body rule not found").toBeTruthy();
    const rule = ruleMatch![1];
    expect(rule, ".log-body must be a flexed, bounded child").toMatch(/min-height\s*:\s*0/);
    expect(rule, ".log-body must be its own scroll region").toMatch(/overflow-y\s*:\s*auto/);
  });
});
