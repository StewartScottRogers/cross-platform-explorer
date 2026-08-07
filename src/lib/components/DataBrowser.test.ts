import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent, within } from "@testing-library/svelte";
import DataBrowser from "./DataBrowser.svelte";

// CPE-1392: jsdom render-spec for the SQLite/Parquet/Excel-ODS data-browser preview (CPE-849). It's a
// thin UI over three typed commands (dataBrowserSources / dataBrowserPage / dataBrowserQuery, backed by
// `data_browser_sources` / `data_browser_page` / `data_browser_query`) — mock those at the
// `@tauri-apps/api/core` boundary, same recipe as DuplicatesDialog.test.ts, since bindings.gen.ts's
// TAURI_INVOKE is `invoke` re-exported from ./invoke, which itself wraps `@tauri-apps/api/core`'s invoke.

interface Col { name: string; type: string }
interface PageResp { columns: Col[]; rows: string[][]; total?: number | null }

let sources: string[] = [];
let sourcesError: string | null = null;
let pageError: string | null = null;
// Keyed by `${source}#${offset}` for table pages, `${sql}#${offset}` for SQL-console pages.
let pages: Record<string, PageResp> = {};
let queryPages: Record<string, PageResp> = {};

const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "data_browser_sources") {
    if (sourcesError) throw new Error(sourcesError);
    return sources;
  }
  if (cmd === "data_browser_page") {
    if (pageError) throw new Error(pageError);
    return pages[`${args.source}#${args.offset}`] ?? { columns: [], rows: [], total: 0 };
  }
  if (cmd === "data_browser_query") {
    if (pageError) throw new Error(pageError);
    return queryPages[`${args.sql}#${args.offset}`] ?? { columns: [], rows: [], total: 0 };
  }
  throw new Error(`unexpected command ${cmd}`);
});
vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

const COLS: Col[] = [
  { name: "id", type: "INTEGER" },
  { name: "name", type: "TEXT" },
];

function rowsOf(n: number, offset: number): string[][] {
  return Array.from({ length: n }, (_, i) => [String(offset + i + 1), `person-${offset + i + 1}`]);
}

// The "rows X–Y of Z" readout is built from adjacent mustache expressions with no space before "of"
// in the template, so Svelte emits it as "…Yof Z" (no space between the count and "of") — read the
// `.db-page` span directly rather than hardcoding that whitespace quirk into a dozen literal strings.
function pageRangeText(): string {
  return document.querySelector(".db-page")?.textContent ?? "";
}

beforeEach(() => {
  sources = [];
  sourcesError = null;
  pageError = null;
  pages = {};
  queryPages = {};
  invoke.mockClear();
});

describe("DataBrowser (CPE-1392)", () => {
  it("loads sources into the table picker and renders the first table's page", async () => {
    sources = ["users", "orders"];
    pages["users#0"] = { columns: COLS, rows: [["1", "Alice"], ["2", "Bob"]], total: 2 };

    render(DataBrowser, { entry: { path: "/data/app.db", extension: "db" } });

    await waitFor(() => expect(screen.getByText("Alice")).toBeTruthy());

    expect(invoke).toHaveBeenCalledWith("data_browser_sources", { path: "/data/app.db" });
    expect(invoke).toHaveBeenCalledWith("data_browser_page", {
      path: "/data/app.db",
      source: "users",
      offset: 0,
      limit: 100,
    });

    // Picker is populated with both sources, defaulted to the first.
    const select = screen.getByTitle("Table / view") as HTMLSelectElement;
    expect(within(select).getAllByRole("option").map((o) => (o as HTMLOptionElement).value)).toEqual([
      "users",
      "orders",
    ]);
    expect(select.value).toBe("users");

    // Column headers + row-range readout.
    expect(screen.getByText("id")).toBeTruthy();
    expect(screen.getByText("name")).toBeTruthy();
    expect(pageRangeText()).toBe("rows 1–2of 2");
  });

  it("switching the source picker reloads that table's page from offset 0", async () => {
    sources = ["users", "orders"];
    pages["users#0"] = { columns: COLS, rows: [["1", "Alice"]], total: 1 };
    pages["orders#0"] = { columns: COLS, rows: [["9", "Widget"]], total: 1 };

    render(DataBrowser, { entry: { path: "/data/app.db", extension: "db" } });
    await waitFor(() => expect(screen.getByText("Alice")).toBeTruthy());
    invoke.mockClear();

    const select = screen.getByTitle("Table / view") as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: "orders" } });

    await waitFor(() => expect(screen.getByText("Widget")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("data_browser_page", {
      path: "/data/app.db",
      source: "orders",
      offset: 0,
      limit: 100,
    });
    expect(screen.queryByText("Alice")).toBeFalsy();
  });

  it("pagination: Next/Prev walk pages and disable at the boundaries", async () => {
    sources = ["users"];
    // Page 1 is a full page (100 rows) so Next is enabled; page 2 is a short final page.
    pages["users#0"] = { columns: COLS, rows: rowsOf(100, 0), total: 105 };
    pages["users#100"] = { columns: COLS, rows: rowsOf(5, 100), total: 105 };

    render(DataBrowser, { entry: { path: "/data/app.db", extension: "db" } });
    await waitFor(() => expect(pageRangeText()).toBe("rows 1–100of 105"));

    const prevBtn = screen.getByRole("button", { name: /Prev/ }) as HTMLButtonElement;
    const nextBtn = screen.getByRole("button", { name: /Next/ }) as HTMLButtonElement;
    expect(prevBtn.disabled).toBe(true);
    expect(nextBtn.disabled).toBe(false);

    await fireEvent.click(nextBtn);
    await waitFor(() => expect(pageRangeText()).toBe("rows 101–105of 105"));
    expect(invoke).toHaveBeenCalledWith("data_browser_page", {
      path: "/data/app.db",
      source: "users",
      offset: 100,
      limit: 100,
    });
    // Short final page (< LIMIT rows): Next disables, Prev enables.
    expect(nextBtn.disabled).toBe(true);
    expect(prevBtn.disabled).toBe(false);

    await fireEvent.click(prevBtn);
    await waitFor(() => expect(pageRangeText()).toBe("rows 1–100of 105"));
    expect(invoke).toHaveBeenCalledWith("data_browser_page", {
      path: "/data/app.db",
      source: "users",
      offset: 0,
      limit: 100,
    });
    expect(prevBtn.disabled).toBe(true);
  });

  it("shows a no-rows state for an empty result page, without erroring", async () => {
    sources = ["empty_table"];
    pages["empty_table#0"] = { columns: COLS, rows: [], total: 0 };

    render(DataBrowser, { entry: { path: "/data/app.db", extension: "db" } });

    await waitFor(() => expect(screen.getByText("No rows.")).toBeTruthy());
    // Header row still renders from the returned columns; there's no error banner.
    expect(screen.getByText("id")).toBeTruthy();
    expect(screen.queryByText(/^(?!No rows\.).*error/i)).toBeFalsy();
  });

  it("a failed SQL query clears the grid and surfaces the error message", async () => {
    sources = ["users"];
    pages["users#0"] = { columns: COLS, rows: [["1", "Alice"]], total: 1 };

    render(DataBrowser, { entry: { path: "/data/app.db", extension: "sqlite" } });
    await waitFor(() => expect(screen.getByText("Alice")).toBeTruthy());

    pageError = "near \"SELCT\": syntax error";
    const sqlInput = screen.getByLabelText("SQL query");
    await fireEvent.input(sqlInput, { target: { value: "SELCT * FROM users" } });
    await fireEvent.click(screen.getByRole("button", { name: "Run" }));

    await waitFor(() => expect(screen.getByText(/syntax error/)).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("data_browser_query", {
      path: "/data/app.db",
      sql: "SELCT * FROM users",
      offset: 0,
      limit: 100,
    });
    // The grid is gone — only the error banner remains.
    expect(screen.queryByText("Alice")).toBeFalsy();
    expect(screen.queryByRole("table")).toBeFalsy();
  });

  it("SQL console runs a read-only query and Clear reverts to the table page", async () => {
    sources = ["users"];
    pages["users#0"] = { columns: COLS, rows: [["1", "Alice"]], total: 1 };
    queryPages["SELECT * FROM users WHERE id > 1#0"] = {
      columns: COLS,
      rows: [["2", "Bob"]],
      total: 1,
    };

    render(DataBrowser, { entry: { path: "/data/app.db", extension: "sqlite" } });
    await waitFor(() => expect(screen.getByText("Alice")).toBeTruthy());

    const sqlInput = screen.getByLabelText("SQL query");
    await fireEvent.input(sqlInput, { target: { value: "SELECT * FROM users WHERE id > 1" } });
    await fireEvent.keyDown(sqlInput, { key: "Enter" });

    await waitFor(() => expect(screen.getByText("Bob")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("data_browser_query", {
      path: "/data/app.db",
      sql: "SELECT * FROM users WHERE id > 1",
      offset: 0,
      limit: 100,
    });
    expect(screen.queryByText("Alice")).toBeFalsy();

    invoke.mockClear();
    await fireEvent.click(screen.getByRole("button", { name: "Clear" }));

    await waitFor(() => expect(screen.getByText("Alice")).toBeTruthy());
    expect(invoke).toHaveBeenCalledWith("data_browser_page", {
      path: "/data/app.db",
      source: "users",
      offset: 0,
      limit: 100,
    });
    // Clear button disappears once sqlMode is off again.
    expect(screen.queryByRole("button", { name: "Clear" })).toBeFalsy();
  });

  it("hides the SQL console for non-SQLite sources and labels the picker 'Sheet'", async () => {
    sources = ["Sheet1"];
    pages["Sheet1#0"] = { columns: COLS, rows: [["1", "Alice"]], total: 1 };

    render(DataBrowser, { entry: { path: "/data/book.xlsx", extension: "xlsx" } });
    await waitFor(() => expect(screen.getByText("Alice")).toBeTruthy());

    expect(screen.getByTitle("Sheet")).toBeTruthy();
    expect(screen.queryByLabelText("SQL query")).toBeFalsy();
  });

  it("filters the current page client-side without re-querying the server", async () => {
    sources = ["users"];
    pages["users#0"] = {
      columns: COLS,
      rows: [["1", "Alice"], ["2", "Bob"]],
      total: 2,
    };

    render(DataBrowser, { entry: { path: "/data/app.db", extension: "db" } });
    await waitFor(() => expect(screen.getByText("Alice")).toBeTruthy());
    invoke.mockClear();

    await fireEvent.input(screen.getByLabelText("Filter rows"), { target: { value: "bob" } });

    await waitFor(() => expect(screen.queryByText("Alice")).toBeFalsy());
    expect(screen.getByText("Bob")).toBeTruthy();
    // Purely client-side — filtering the already-fetched page issues no new backend call.
    expect(invoke).not.toHaveBeenCalled();
  });
});
