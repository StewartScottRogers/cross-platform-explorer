/**
 * ShredConfirmDialog (CPE-1240, epic CPE-738): the honest confirm dialog gating `shred_paths`, the
 * one destructive op with NO trash fallback. Checks the required copy is present (permanence +
 * platform caveat), the scheme picker + danger-button confirm wiring, and the done/error dispatch
 * contract. Mocks the Tauri `invoke` boundary, same convention as RepairLinkDialog.test.ts.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import ShredConfirmDialog from "./ShredConfirmDialog.svelte";

let shredOk = true;
const invoke = vi.fn(async (cmd: string, args?: any) => {
  if (cmd === "shred_paths") {
    if (!shredOk) throw new Error("cannot open file for pass: os error 5");
    return (args.paths as string[]).map((p) => ({
      path: p,
      ok: true,
      error: "",
      passes_run: args.scheme === "dod_3" ? 3 : 1,
      bytes_written: 1024,
      removed: true,
    }));
  }
  throw new Error(`unexpected invoke: ${cmd}`);
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));

const base = { paths: ["/repo/secret.txt"], what: '"secret.txt"' };

describe("ShredConfirmDialog honest copy (CPE-1240)", () => {
  it("states permanence: PERMANENT/non-recoverable and NOT the Recycle Bin/Trash", () => {
    render(ShredConfirmDialog, base);
    const text = screen.getByTestId("shred-permanence").textContent ?? "";
    expect(text.toLowerCase()).toContain("permanent");
    expect(text.toLowerCase()).toContain("non-recoverable");
    expect(text).toContain("Recycle Bin");
    expect(text.toLowerCase()).toContain("does");
    expect(text.toLowerCase()).toContain("not");
  });

  it("states the honest platform caveat: best-effort, not a guarantee, with SSD + copy-on-write specifics", () => {
    render(ShredConfirmDialog, base);
    const text = screen.getByTestId("shred-caveat").textContent ?? "";
    expect(text.toLowerCase()).toContain("best-effort");
    expect(text.toLowerCase()).toContain("not a guarantee");
    expect(text.toLowerCase()).toContain("ssd");
    expect(text.toLowerCase()).toContain("copy-on-write");
  });

  it("makes no false erasure guarantee anywhere in the dialog body", () => {
    render(ShredConfirmDialog, base);
    const dialogText = (screen.getByRole("dialog").textContent ?? "").toLowerCase();
    // Never claim the shred itself definitely erases the data — only that overwriting is best-effort.
    // ("For guaranteed erasure, use full-disk encryption…" is fine: it recommends a DIFFERENT, actually-
    // guaranteed remedy rather than overclaiming what shredding itself does.)
    expect(dialogText).not.toContain("completely erased");
    expect(dialogText).not.toContain("fully erased");
    expect(dialogText).not.toContain("guaranteed to be unrecoverable");
    expect(dialogText).not.toContain("this guarantees");
  });

  it("shows the title referencing what will be shredded", () => {
    render(ShredConfirmDialog, base);
    expect(screen.getByText(/Securely delete "secret\.txt"\?/)).toBeTruthy();
  });
});

describe("ShredConfirmDialog wiring (CPE-1240)", () => {
  it("offers all four overwrite schemes, defaulting to zero-fill", () => {
    render(ShredConfirmDialog, base);
    const select = screen.getByTestId("shred-scheme") as HTMLSelectElement;
    const values = Array.from(select.options).map((o) => o.value);
    expect(values).toEqual(["zero", "random", "dod_3", "gutmann"]);
    expect(select.value).toBe("zero");
  });

  it("cancel dispatches close without calling shred_paths", async () => {
    invoke.mockClear();
    const { component } = render(ShredConfirmDialog, base);
    const close = vi.fn();
    component.$on("close", close);

    await fireEvent.click(screen.getByTestId("shred-cancel"));

    expect(close).toHaveBeenCalledTimes(1);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("confirming calls shred_paths with the given paths + selected scheme, and dispatches done with the results", async () => {
    shredOk = true;
    invoke.mockClear();
    const { component } = render(ShredConfirmDialog, base);
    const done = vi.fn();
    component.$on("done", (e: CustomEvent<any[]>) => done(e.detail));

    await fireEvent.change(screen.getByTestId("shred-scheme"), { target: { value: "dod_3" } });
    await fireEvent.click(screen.getByTestId("shred-confirm"));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("shred_paths", {
      paths: ["/repo/secret.txt"],
      scheme: "dod_3",
      confirmed: true,
    }));
    await waitFor(() => expect(done).toHaveBeenCalledTimes(1));
    expect(done.mock.calls[0][0]).toEqual([
      { path: "/repo/secret.txt", ok: true, error: "", passes_run: 3, bytes_written: 1024, removed: true },
    ]);
  });

  it("a failed shred surfaces via the dispatched error event and keeps the dialog open (inline error shown)", async () => {
    shredOk = false;
    invoke.mockClear();
    const { component } = render(ShredConfirmDialog, base);
    const errorSpy = vi.fn();
    const done = vi.fn();
    component.$on("error", (e: CustomEvent<string>) => errorSpy(e.detail));
    component.$on("done", done);

    await fireEvent.click(screen.getByTestId("shred-confirm"));

    await waitFor(() => expect(errorSpy).toHaveBeenCalledTimes(1));
    expect(errorSpy.mock.calls[0][0]).toContain("os error 5");
    expect(screen.getByTestId("shred-error")).toBeTruthy();
    expect(done).not.toHaveBeenCalled();
    // Still open — Cancel is still there to dismiss (not auto-closed on failure).
    expect(screen.getByTestId("shred-cancel")).toBeTruthy();
    shredOk = true; // restore for any later tests
  });

  it("the confirm button is a clearly-labelled destructive action ('Shred permanently')", () => {
    render(ShredConfirmDialog, base);
    expect(screen.getByTestId("shred-confirm").textContent).toContain("Shred permanently");
  });
});
