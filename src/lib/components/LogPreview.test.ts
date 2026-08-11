import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import LogPreview from "./LogPreview.svelte";
import { MAX_LINES } from "../preview/logViewer";
import type { LogWindow } from "../bindings.gen";

// CPE-1618 (epic CPE-1568 slice 8), extended by CPE-1637 (bounded windowed reads): jsdom render-spec for
// the log preview, wiring the `read_log_window` backend command into a standalone component (same mocking
// recipe as NotebookPreview.test.ts/CertPreview.test.ts: mock `../bindings.gen`'s `commands` object). jsdom
// can't see layout, so these assert text/DOM content, filter behaviour, and robustness paths only — never a
// visual verdict (see the dedicated structural CSS guard at the bottom for the bounded-scroll claim).

const { readLogWindowMock } = vi.hoisted(() => ({ readLogWindowMock: vi.fn() }));

vi.mock("../bindings.gen", () => ({
  commands: { readLogWindow: readLogWindowMock },
}));

function ok(win: LogWindow) {
  return { status: "ok" as const, data: win };
}

/** A `LogWindow` covering the whole (small) file in one read — `at_start`/`at_end` both true, matching
 *  the pre-CPE-1637 "load the whole file" behavior exactly, so most existing tests need no shape change
 *  beyond the mock's return type. */
function wholeFile(text: string): LogWindow {
  const len = text.length;
  return {
    text,
    window_start: 0,
    window_end: len,
    file_len: len,
    at_start: true,
    at_end: true,
    line_aligned: true,
  };
}

beforeEach(() => {
  readLogWindowMock.mockReset();
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
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile(MIXED_LOG)));

    const { container } = render(LogPreview, { path: "/x/app.log" });

    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(5));
    expect(readLogWindowMock).toHaveBeenCalledWith("/x/app.log", expect.any(Number), null);

    const rows = Array.from(container.querySelectorAll('[data-testid="log-row"]'));
    expect(rows.map((r) => r.getAttribute("data-level"))).toEqual(["info", "warn", "error", "debug", "none"]);
    expect(container.textContent).toContain("Failed to connect to database");
  });

  it("shows per-level counts in the filter chips", async () => {
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile(MIXED_LOG)));

    const { container } = render(LogPreview, { path: "/x/app.log" });

    await waitFor(() => expect(container.querySelector('[data-testid="log-filter-chip-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="log-filter-chip-error"]')!.textContent).toContain("1");
    expect(container.querySelector('[data-testid="log-filter-chip-info"]')!.textContent).toContain("1");
  });

  it("filter chips actually hide non-matching rows and the visible count updates", async () => {
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile(MIXED_LOG)));

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
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile(MIXED_LOG)));

    const { container } = render(LogPreview, { path: "/x/app.log" });
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(5));

    const errorChip = container.querySelector('[data-testid="log-filter-chip-error"]')!;
    await fireEvent.click(errorChip); // off
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(4));
    await fireEvent.click(errorChip); // back on
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(5));
  });

  it("does not misclassify a line that merely mentions 'error' in prose (end-to-end through the component)", async () => {
    readLogWindowMock.mockResolvedValueOnce(
      ok(wholeFile("User asked about a checkout error they saw yesterday.\nINFO normal line")),
    );

    const { container } = render(LogPreview, { path: "/x/app.log" });
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(2));

    const rows = Array.from(container.querySelectorAll('[data-testid="log-row"]'));
    expect(rows[0].getAttribute("data-level")).toBe("none");
    expect(rows[1].getAttribute("data-level")).toBe("info");
  });

  it("strips ANSI escape codes end-to-end so no literal escape-code garbage reaches the DOM", async () => {
    const esc = String.fromCharCode(27);
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile(esc + "[31mERROR" + esc + "[0m payment failed")));

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
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile(text)));

    const { container } = render(LogPreview, { path: "/x/huge.log" });

    await waitFor(() => expect(container.querySelector('[data-testid="log-lines-capped"]')).toBeTruthy());
    expect(container.querySelectorAll('[data-testid="log-row"]').length).toBeLessThanOrEqual(MAX_LINES);
    expect(container.querySelector('[data-testid="log-lines-capped"]')!.textContent).toMatch(
      new RegExp(`of ${totalLines.toLocaleString()}`),
    );
  });

  it("shows a load-error state when the invoke call itself fails, without crashing", async () => {
    readLogWindowMock.mockRejectedValueOnce(new Error("File is too large to preview"));

    const { container } = render(LogPreview, { path: "/x/huge.log" });

    await waitFor(() => expect(container.querySelector('[data-testid="log-load-error"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="log-load-error"]')!.textContent).toContain("too large");
  });

  it("handles an empty file without crashing, rendering a clear 'empty' note rather than nothing at all", async () => {
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile("")));

    const { container } = render(LogPreview, { path: "/x/empty.log" });

    await waitFor(() => expect(container.textContent).toMatch(/empty/i));
    expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(0);
    // Empty must never be confused with the windowed/partial-view note.
    expect(container.querySelector('[data-testid="log-window-note"]')).toBeNull();
  });

  it("filter chips carry aria-pressed reflecting whether that level is currently active (CPE-1618 Visual Critic)", async () => {
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile(MIXED_LOG)));

    const { container } = render(LogPreview, { path: "/x/app.log" });
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(5));

    const errorChip = container.querySelector('[data-testid="log-filter-chip-error"]')!;
    const unleveledChip = container.querySelector('[data-testid="log-filter-chip-unleveled"]')!;

    // All levels + "Other" start active.
    for (const level of ["error", "warn", "info", "debug", "trace"]) {
      expect(container.querySelector(`[data-testid="log-filter-chip-${level}"]`)!.getAttribute("aria-pressed")).toBe(
        "true",
      );
    }
    expect(unleveledChip.getAttribute("aria-pressed")).toBe("true");

    await fireEvent.click(errorChip);
    expect(errorChip.getAttribute("aria-pressed")).toBe("false");

    await fireEvent.click(errorChip);
    expect(errorChip.getAttribute("aria-pressed")).toBe("true");

    await fireEvent.click(unleveledChip);
    expect(unleveledChip.getAttribute("aria-pressed")).toBe("false");
  });

  it("the 'Showing N of M' count is a live region so a screen reader hears filter changes (CPE-1618 Visual Critic)", async () => {
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile(MIXED_LOG)));

    const { container } = render(LogPreview, { path: "/x/app.log" });
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(5));

    const count = container.querySelector('[data-testid="log-visible-count"]')!;
    expect(count.getAttribute("aria-live")).toBe("polite");
  });

  it("the log body is a single, focusable, keyboard-scrollable tab stop (CPE-1618 Visual Critic)", async () => {
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile(MIXED_LOG)));

    const { container } = render(LogPreview, { path: "/x/app.log" });
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(5));

    const body = container.querySelector('[data-testid="log-body"]')! as HTMLElement;
    expect(body.getAttribute("tabindex")).toBe("0");
    body.focus();
    expect(document.activeElement).toBe(body);

    // Individual rows must NOT be extra tab stops — only the region itself.
    const rows = Array.from(container.querySelectorAll('[data-testid="log-row"]'));
    for (const row of rows) expect(row.getAttribute("tabindex")).toBeNull();
  });

  it("switching to a new path re-loads and replaces the previous file's rows", async () => {
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile("ERROR first file")));
    const { container, rerender } = render(LogPreview, { path: "/x/a.log" });
    await waitFor(() => expect(container.textContent).toContain("first file"));

    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile("INFO second file")));
    await rerender({ path: "/x/b.log" });
    await waitFor(() => expect(container.textContent).toContain("second file"));
    expect(container.textContent).not.toContain("first file");
  });
});

// CPE-1637: bounded windowed reads for multi-megabyte real logs (CBS.log, dism.log, …) that the old
// all-or-nothing `read_file_text` refused outright above its 256 KiB cap. These tests exercise the
// tail-window note, the "Load earlier" / "Back to latest" paging controls, and that paging never
// re-reads bytes it already has.
describe("LogPreview windowed/partial reads (CPE-1637)", () => {
  /** A window that is only the TAIL of a much larger file — `at_end` true, `at_start` false. */
  function tailWindow(text: string, fileLen: number): LogWindow {
    return {
      text,
      window_start: fileLen - text.length,
      window_end: fileLen,
      file_len: fileLen,
      at_start: false,
      at_end: true,
      line_aligned: true,
    };
  }

  it("a small file that fits in one window shows no windowed note or paging controls (unchanged behavior)", async () => {
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile(MIXED_LOG)));

    const { container } = render(LogPreview, { path: "/x/small.log" });
    await waitFor(() => expect(container.querySelectorAll('[data-testid="log-row"]').length).toBe(5));

    expect(container.querySelector('[data-testid="log-window-note"]')).toBeNull();
    expect(container.querySelector('[data-testid="log-page-controls"]')).toBeNull();
  });

  it("a tail window of a huge file states precisely which part is shown, and offers 'Load earlier'", async () => {
    const text = "line A\nline B\nline C\n";
    readLogWindowMock.mockResolvedValueOnce(ok(tailWindow(text, 15_000_000)));

    const { container } = render(LogPreview, { path: "/x/CBS.log" });
    await waitFor(() => expect(container.querySelector('[data-testid="log-window-note"]')).toBeTruthy());

    const note = container.querySelector('[data-testid="log-window-note"]')!.textContent!;
    expect(note).toMatch(/last/i);
    expect(note).not.toMatch(/too large/i); // never the old outright-refusal wording

    const older = container.querySelector('[data-testid="log-load-older"]') as HTMLButtonElement;
    expect(older).toBeTruthy();
    expect(older.disabled).toBe(false);
    // Already at the tail: no "back to latest" needed.
    expect(container.querySelector('[data-testid="log-jump-latest"]')).toBeNull();
  });

  it("'Load earlier' issues exactly one bounded read ending at the current window's aligned start, not a wider re-read", async () => {
    const tailText = "line A\nline B\nline C\n";
    const tail = tailWindow(tailText, 15_000_000);
    readLogWindowMock.mockResolvedValueOnce(ok(tail));

    const { container } = render(LogPreview, { path: "/x/CBS.log" });
    await waitFor(() => expect(container.querySelector('[data-testid="log-load-older"]')).toBeTruthy());

    const olderText = "prior line X\nprior line Y\n";
    const older: LogWindow = {
      text: olderText,
      window_start: tail.window_start - olderText.length,
      window_end: tail.window_start,
      file_len: tail.file_len,
      at_start: false,
      at_end: false,
      line_aligned: true,
    };
    readLogWindowMock.mockResolvedValueOnce(ok(older));

    await fireEvent.click(container.querySelector('[data-testid="log-load-older"]')!);

    await waitFor(() => expect(container.textContent).toContain("prior line X"));
    expect(readLogWindowMock).toHaveBeenLastCalledWith("/x/CBS.log", expect.any(Number), tail.window_start);
    expect(readLogWindowMock).toHaveBeenCalledTimes(2);

    // Now mid-file: neither at the true start nor the tail, so both paging controls are offered, and the
    // note says so honestly rather than claiming either extreme.
    const note = container.querySelector('[data-testid="log-window-note"]')!.textContent!;
    expect(note).toMatch(/not the end of the file/i);
    expect(container.querySelector('[data-testid="log-jump-latest"]')).toBeTruthy();
  });

  it("'Back to latest' returns to the cached tail page without issuing another backend read", async () => {
    const tailText = "line A\nline B\n";
    const tail = tailWindow(tailText, 5_000_000);
    readLogWindowMock.mockResolvedValueOnce(ok(tail));

    const { container } = render(LogPreview, { path: "/x/dism.log" });
    await waitFor(() => expect(container.querySelector('[data-testid="log-load-older"]')).toBeTruthy());

    const olderText = "older line\n";
    const older: LogWindow = {
      text: olderText,
      window_start: tail.window_start - olderText.length,
      window_end: tail.window_start,
      file_len: tail.file_len,
      at_start: false,
      at_end: false,
      line_aligned: true,
    };
    readLogWindowMock.mockResolvedValueOnce(ok(older));
    await fireEvent.click(container.querySelector('[data-testid="log-load-older"]')!);
    await waitFor(() => expect(container.textContent).toContain("older line"));
    expect(readLogWindowMock).toHaveBeenCalledTimes(2);

    await fireEvent.click(container.querySelector('[data-testid="log-jump-latest"]')!);
    await waitFor(() => expect(container.textContent).toContain("line A"));
    expect(container.textContent).not.toContain("older line");
    // Still only the two backend calls from before — the tail page was cached, not re-fetched.
    expect(readLogWindowMock).toHaveBeenCalledTimes(2);
  });

  it("reaching the true start of the file disables 'Load earlier' and drops the 'not the end' wording", async () => {
    const text = "only line\n";
    const atStart: LogWindow = {
      text,
      window_start: 0,
      window_end: text.length,
      file_len: 9_000_000,
      at_start: true,
      at_end: false,
      line_aligned: true,
    };
    readLogWindowMock.mockResolvedValueOnce(ok(atStart));

    const { container } = render(LogPreview, { path: "/x/huge.log" });
    await waitFor(() => expect(container.querySelector('[data-testid="log-window-note"]')).toBeTruthy());

    const older = container.querySelector('[data-testid="log-load-older"]') as HTMLButtonElement;
    expect(older.disabled).toBe(true);
    expect(container.querySelector('[data-testid="log-jump-latest"]')).toBeTruthy();
  });

  it("flags a window with no line break as not line-aligned, distinct from the normal windowed note", async () => {
    const text = "x".repeat(500); // one giant line, no '\n' at all
    const unaligned: LogWindow = {
      text,
      window_start: 12_345,
      window_end: 12_345 + text.length,
      file_len: 9_000_000,
      at_start: false,
      at_end: false,
      line_aligned: false,
    };
    readLogWindowMock.mockResolvedValueOnce(ok(unaligned));

    const { container } = render(LogPreview, { path: "/x/oneline.log" });
    await waitFor(() => expect(container.querySelector('[data-testid="log-window-note"]')).toBeTruthy());
    expect(container.querySelector('[data-testid="log-window-note"]')!.textContent).toMatch(/mid-line/i);
  });

  it("a read failure, an empty file, and a partial windowed view remain three visibly distinct states", async () => {
    // 1. Read failure.
    readLogWindowMock.mockRejectedValueOnce(new Error("permission denied"));
    const err = render(LogPreview, { path: "/x/fail.log" });
    await waitFor(() => expect(err.container.querySelector('[data-testid="log-load-error"]')).toBeTruthy());
    expect(err.container.querySelector('[data-testid="log-window-note"]')).toBeNull();
    err.unmount();

    // 2. Empty file.
    readLogWindowMock.mockResolvedValueOnce(ok(wholeFile("")));
    const empty = render(LogPreview, { path: "/x/empty.log" });
    await waitFor(() => expect(empty.container.textContent).toMatch(/empty/i));
    expect(empty.container.querySelector('[data-testid="log-load-error"]')).toBeNull();
    expect(empty.container.querySelector('[data-testid="log-window-note"]')).toBeNull();
    empty.unmount();

    // 3. Partial windowed view.
    readLogWindowMock.mockResolvedValueOnce(ok(tailWindow("line A\nline B\n", 12_000_000)));
    const partial = render(LogPreview, { path: "/x/CBS.log" });
    await waitFor(() => expect(partial.container.querySelector('[data-testid="log-window-note"]')).toBeTruthy());
    expect(partial.container.querySelector('[data-testid="log-load-error"]')).toBeNull();
    expect(partial.container.textContent).not.toMatch(/this file is empty/i);
    partial.unmount();
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
