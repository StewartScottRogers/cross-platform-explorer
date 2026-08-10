/**
 * DropStackPanel component test (CPE-1532, epic CPE-1489). The store itself (`dropStack.ts`, CPE-1530)
 * is already unit-tested headlessly (pure reducers + the writable tail); this test mounts the real
 * Svelte component against a mocked `../dropStack` module to prove the panel actually renders the
 * stack as pills and wires the remove/clear-all controls to the store's functions — mirroring
 * TerminalPanel.test.ts's "mount the real component on a mocked module" pattern.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, cleanup } from "@testing-library/svelte";
import { writable } from "svelte/store";
import type { DropStackEntry } from "../dropStack";

const removeFromDropStack = vi.fn();
const clearDropStack = vi.fn();
const store = writable<DropStackEntry[]>([]);

vi.mock("../dropStack", () => ({
  get dropStackEntries() {
    return store;
  },
  removeFromDropStack: (path: string) => removeFromDropStack(path),
  clearDropStack: () => clearDropStack(),
}));

// Imported AFTER the mock is registered (vi.mock is hoisted by vitest, so this is safe either way).
import DropStackPanel from "./DropStackPanel.svelte";

function entry(path: string, addedFrom = "/repo"): DropStackEntry {
  return { path, addedFrom, addedAt: 0 };
}

beforeEach(() => {
  removeFromDropStack.mockClear();
  clearDropStack.mockClear();
  store.set([]);
});

afterEach(() => cleanup());

describe("DropStackPanel (CPE-1532)", () => {
  it("is collapsed by default — only the toggle handle shows, no list", () => {
    store.set([entry("/repo/notes.txt")]);
    const { getByLabelText, queryByRole } = render(DropStackPanel);

    expect(getByLabelText("Show Drop Stack")).toBeTruthy();
    expect(queryByRole("region", { name: "Drop Stack" })).toBeNull();
  });

  it("the handle shows a count badge once something is shelved", () => {
    store.set([entry("/repo/a.txt"), entry("/repo/b.txt")]);
    const { getByLabelText } = render(DropStackPanel);
    expect(getByLabelText("Show Drop Stack").textContent).toContain("2");
  });

  it("opening the panel renders the current stack entries as pills", async () => {
    store.set([entry("/repo/notes.txt"), entry("/repo/photos/beach.png")]);
    const { getByLabelText, getByTitle } = render(DropStackPanel);

    await fireEvent.click(getByLabelText("Show Drop Stack"));

    expect(getByTitle("/repo/notes.txt")).toBeTruthy();
    expect(getByTitle("/repo/photos/beach.png")).toBeTruthy();
  });

  it("empty state renders the hint instead of a pill list", async () => {
    const { getByLabelText, getByText } = render(DropStackPanel);

    await fireEvent.click(getByLabelText("Show Drop Stack"));

    expect(getByText(/Drop Stack is empty/)).toBeTruthy();
    expect(getByText(/Add to Drop Stack/)).toBeTruthy();
  });

  it("a pill's remove button calls removeFromDropStack with that entry's path", async () => {
    store.set([entry("/repo/notes.txt")]);
    const { getByLabelText } = render(DropStackPanel);

    await fireEvent.click(getByLabelText("Show Drop Stack"));
    await fireEvent.click(getByLabelText("Remove notes.txt from Drop Stack"));

    expect(removeFromDropStack).toHaveBeenCalledTimes(1);
    expect(removeFromDropStack).toHaveBeenCalledWith("/repo/notes.txt");
  });

  it("Clear all calls clearDropStack", async () => {
    store.set([entry("/repo/a.txt"), entry("/repo/b.txt")]);
    const { getByLabelText, getByText } = render(DropStackPanel);

    await fireEvent.click(getByLabelText("Show Drop Stack"));
    await fireEvent.click(getByText("Clear all"));

    expect(clearDropStack).toHaveBeenCalledTimes(1);
  });

  it("Clear all is not shown when the stack is empty", async () => {
    const { getByLabelText, queryByText } = render(DropStackPanel);
    await fireEvent.click(getByLabelText("Show Drop Stack"));
    expect(queryByText("Clear all")).toBeNull();
  });

  it("a pill's visible label is the filename, not the full path — the full path is on the title tooltip so long paths never overflow the pill (tick-tack reflow contract)", async () => {
    const longPath = "/repo/very/deeply/nested/folder/structure/that/keeps/going/final-report.docx";
    store.set([entry(longPath)]);
    const { getByLabelText, getByTitle, queryByText } = render(DropStackPanel);

    await fireEvent.click(getByLabelText("Show Drop Stack"));

    const pill = getByTitle(longPath);
    expect(pill.textContent).toContain("final-report.docx");
    expect(queryByText(longPath)).toBeNull(); // the full path is never rendered as pill text
  });

  it("the panel's own close (x) button closes it again without touching the store", async () => {
    store.set([entry("/repo/notes.txt")]);
    const { getByLabelText, queryByRole } = render(DropStackPanel);

    await fireEvent.click(getByLabelText("Show Drop Stack"));
    expect(queryByRole("region", { name: "Drop Stack" })).toBeTruthy();

    await fireEvent.click(getByLabelText("Close Drop Stack panel"));

    expect(queryByRole("region", { name: "Drop Stack" })).toBeNull();
    expect(removeFromDropStack).not.toHaveBeenCalled();
    expect(clearDropStack).not.toHaveBeenCalled();
  });
});

describe("DropStackPanel — Move-all/Copy-all (CPE-1533)", () => {
  it("hides both buttons when the stack is empty, alongside Clear all", async () => {
    const { getByLabelText, queryByText } = render(DropStackPanel);
    await fireEvent.click(getByLabelText("Show Drop Stack"));

    expect(queryByText("Move all here")).toBeNull();
    expect(queryByText("Copy all here")).toBeNull();
  });

  it("shows both buttons, enabled, once something is shelved and the destination is valid", async () => {
    store.set([entry("/repo/notes.txt")]);
    const { getByLabelText, getByText } = render(DropStackPanel, { props: { canTransfer: true } });
    await fireEvent.click(getByLabelText("Show Drop Stack"));

    const moveBtn = getByText("Move all here") as HTMLButtonElement;
    const copyBtn = getByText("Copy all here") as HTMLButtonElement;
    expect(moveBtn.disabled).toBe(false);
    expect(copyBtn.disabled).toBe(false);
  });

  it("disables both buttons when canTransfer is false, even with a non-empty stack", async () => {
    store.set([entry("/repo/notes.txt")]);
    const { getByLabelText, getByText } = render(DropStackPanel, { props: { canTransfer: false } });
    await fireEvent.click(getByLabelText("Show Drop Stack"));

    const moveBtn = getByText("Move all here") as HTMLButtonElement;
    const copyBtn = getByText("Copy all here") as HTMLButtonElement;
    expect(moveBtn.disabled).toBe(true);
    expect(copyBtn.disabled).toBe(true);
  });

  it("clicking Move all here dispatches a moveAll event", async () => {
    store.set([entry("/repo/notes.txt"), entry("/repo/b.txt")]);
    const { getByLabelText, getByText, component } = render(DropStackPanel, { props: { canTransfer: true } });
    const moveAll = vi.fn();
    component.$on("moveAll", moveAll);

    await fireEvent.click(getByLabelText("Show Drop Stack"));
    await fireEvent.click(getByText("Move all here"));

    expect(moveAll).toHaveBeenCalledTimes(1);
  });

  it("clicking Copy all here dispatches a copyAll event", async () => {
    store.set([entry("/repo/notes.txt"), entry("/repo/b.txt")]);
    const { getByLabelText, getByText, component } = render(DropStackPanel, { props: { canTransfer: true } });
    const copyAll = vi.fn();
    component.$on("copyAll", copyAll);

    await fireEvent.click(getByLabelText("Show Drop Stack"));
    await fireEvent.click(getByText("Copy all here"));

    expect(copyAll).toHaveBeenCalledTimes(1);
  });

  it("a disabled Move all here does not dispatch moveAll when clicked", async () => {
    store.set([entry("/repo/notes.txt")]);
    const { getByLabelText, getByText, component } = render(DropStackPanel, { props: { canTransfer: false } });
    const moveAll = vi.fn();
    component.$on("moveAll", moveAll);

    await fireEvent.click(getByLabelText("Show Drop Stack"));
    await fireEvent.click(getByText("Move all here"));

    expect(moveAll).not.toHaveBeenCalled();
  });
});
