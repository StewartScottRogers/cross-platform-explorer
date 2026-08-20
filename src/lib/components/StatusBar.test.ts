/**
 * Component tests for the StatusBar item count, including the "X of Y items" filtered readout
 * (CPE-407) and the free-space readout (CPE-403).
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import StatusBar from "./StatusBar.svelte";

describe("StatusBar item count (CPE-407)", () => {
  it("shows a plain count when nothing is filtered", () => {
    render(StatusBar, { itemCount: 5, totalCount: 5 });
    expect(screen.getByText("5 items")).toBeTruthy();
  });

  it("shows 'X of Y items' when a filter is narrowing the list", () => {
    render(StatusBar, { itemCount: 3, totalCount: 12 });
    expect(screen.getByText("3 of 12 items")).toBeTruthy();
  });

  it("uses the singular for a single item", () => {
    render(StatusBar, { itemCount: 1, totalCount: 1 });
    expect(screen.getByText("1 item")).toBeTruthy();
  });
});

describe("StatusBar free space (CPE-403)", () => {
  it("renders free/total when known", () => {
    render(StatusBar, { itemCount: 1, totalCount: 1, diskFree: 50 * 1024 ** 3, diskTotal: 200 * 1024 ** 3 });
    expect(screen.getByText(/50.0 GB free of 200.0 GB/)).toBeTruthy();
  });
  it("omits free space when unknown", () => {
    render(StatusBar, { itemCount: 1, totalCount: 1, diskFree: null, diskTotal: null });
    expect(screen.queryByText(/free of/)).toBeNull();
  });
});

describe("StatusBar filtered-hidden note (CPE-1708)", () => {
  it("shows nothing when nothing was filtered — the overwhelming majority of listings", () => {
    render(StatusBar, { itemCount: 5, totalCount: 5, filteredHidden: 0 });
    expect(screen.queryByText(/hidden because/)).toBeNull();
  });

  it("says N entries were hidden, for a person, when the count is non-zero", () => {
    render(StatusBar, { itemCount: 5, totalCount: 5, filteredHidden: 3 });
    expect(
      screen.getByText("3 entries were hidden because their names could not be shown safely"),
    ).toBeTruthy();
  });

  it("uses the singular for exactly one", () => {
    render(StatusBar, { itemCount: 5, totalCount: 5, filteredHidden: 1 });
    expect(
      screen.getByText("1 entry was hidden because its name could not be shown safely"),
    ).toBeTruthy();
  });

  it("never implies the listing itself failed — no error-tinted wording", () => {
    render(StatusBar, { itemCount: 5, totalCount: 5, filteredHidden: 2 });
    expect(screen.queryByText(/fail|error|could not load|couldn'?t open/i)).toBeNull();
  });

  // Foreman F2 (regression): the visible text truncates to an ellipsis at narrow widths
  // (`.filtered-hidden`'s CSS, mirroring `.notice`'s CPE-1660 overflow strategy) — so the reader's
  // only way to recover the full sentence is the `title` tooltip. It must therefore carry the SAME
  // sentence the body shows (not a different, shorter, fixed string), plus the "loaded successfully"
  // reassurance.
  it("puts the full sentence — plus the loaded-successfully reassurance — in the title tooltip, not a different fixed string", () => {
    render(StatusBar, { itemCount: 5, totalCount: 5, filteredHidden: 3 });
    const note = screen.getByText("3 entries were hidden because their names could not be shown safely");
    expect(note.getAttribute("title")).toContain(
      "3 entries were hidden because their names could not be shown safely",
    );
    expect(note.getAttribute("title")).toContain("loaded successfully");
  });
});

// CPE-1798: `notice` is fed backend error strings from 35 `showNotice(String(e), true)` call sites in
// App.svelte, and a Rust error routinely embeds the offending filesystem path — so a bidi/format-control
// character sitting in that path must not reach either render position raw. Reproduced with a real RLO
// override (U+202E), the same character `src/lib/filename.ts`'s own doc comment uses as its example.
describe("StatusBar notice bidi escape (CPE-1798)", () => {
  it("escapes a bidi override embedded in the notice's path, in both the visible text and the title tooltip", () => {
    const spoofedPath = "C:\\Users\\demo\\\u202Egnp.txt.exe";
    render(StatusBar, { itemCount: 1, totalCount: 1, notice: `Delete failed: ${spoofedPath}`, noticeIsError: true });
    const note = screen.getByText(`Delete failed: C:\\Users\\demo\\[RLO]gnp.txt.exe`);
    expect(note).toBeTruthy();
    expect(note.textContent).not.toContain("\u202E");
    expect(note.getAttribute("title")).toBe(`Delete failed: C:\\Users\\demo\\[RLO]gnp.txt.exe`);
    expect(note.getAttribute("title")).not.toContain("\u202E");
  });

  it("leaves an ordinary notice untouched (idempotent no-op on plain prose)", () => {
    render(StatusBar, { itemCount: 1, totalCount: 1, notice: "Copied 3 files", noticeIsError: false });
    expect(screen.getByText("Copied 3 files")).toBeTruthy();
  });
});

describe("StatusBar git sync (CPE-462)", () => {
  it("shows branch + ahead/behind and offers Pull/Push, dispatching them", async () => {
    const { component } = render(StatusBar, {
      itemCount: 1, totalCount: 1,
      git: { is_repo: true, branch: "main", upstream: "origin/main", ahead: 2, behind: 1, dirty: true },
    });
    expect(screen.getByText(/⎇ main/)).toBeTruthy();
    expect(screen.getByText("↑2")).toBeTruthy();
    expect(screen.getByText("↓1")).toBeTruthy();

    const pull = vi.fn(); const push = vi.fn();
    component.$on("pull", pull); component.$on("push", push);
    await fireEvent.click(screen.getByRole("button", { name: /pull/i }));
    await fireEvent.click(screen.getByRole("button", { name: /push/i }));
    expect(pull).toHaveBeenCalledOnce();
    expect(push).toHaveBeenCalledOnce();
  });

  it("shows nothing git-related when the folder isn't a repo", () => {
    render(StatusBar, { itemCount: 1, totalCount: 1, git: null });
    expect(screen.queryByText(/⎇/)).toBeNull();
    expect(screen.queryByRole("button", { name: /pull|push/i })).toBeNull();
  });

  it("offers only Push when ahead-only (nothing to pull)", () => {
    render(StatusBar, { itemCount: 1, totalCount: 1, git: { is_repo: true, branch: "main", ahead: 1, behind: 0 } });
    expect(screen.getByRole("button", { name: /push/i })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /pull/i })).toBeNull();
  });
});
