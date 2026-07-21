/**
 * Component tests for the attributes editor's batch/timestamp/preview behavior (CPE-786). The backend
 * commands are mocked; these assert the dialog reads each target, renders the batch UI + timestamp input,
 * previews changes, and applies edited fields to every target.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import AttributesDialog from "./AttributesDialog.svelte";

const calls: { cmd: string; args: Record<string, unknown> }[] = [];
const winAttrs = { readonly: false, hidden: false, system: false, archive: false, mode: null };

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown>) => {
    calls.push({ cmd, args });
    if (cmd === "read_attributes") return { ...winAttrs };
    return undefined;
  }),
}));

const targets = [
  { path: "C:\\a.txt", name: "a.txt", modifiedMs: new Date(2026, 5, 1, 12, 0).getTime() },
  { path: "C:\\b.txt", name: "b.txt", modifiedMs: new Date(2026, 5, 1, 12, 0).getTime() },
];

beforeEach(() => (calls.length = 0));

describe("AttributesDialog batch (CPE-786)", () => {
  it("reads every target and shows the batch heading + note + timestamp input", async () => {
    render(AttributesDialog, { targets });
    await waitFor(() => expect(screen.getByTestId("attr-grid")).toBeTruthy());
    expect(screen.getByText("Attributes — 2 items")).toBeTruthy();
    expect(screen.getByText(/apply to all 2 selected/i)).toBeTruthy();
    expect(screen.getByTestId("attr-modified")).toBeTruthy();
    // both targets were read
    expect(calls.filter((c) => c.cmd === "read_attributes").map((c) => c.args.path)).toEqual(["C:\\a.txt", "C:\\b.txt"]);
  });

  it("previews a changed field and applies it to every target", async () => {
    render(AttributesDialog, { targets });
    await waitFor(() => expect(screen.getByTestId("attr-readonly")).toBeTruthy());

    await fireEvent.click(screen.getByTestId("attr-readonly")); // turn Read-only on
    await waitFor(() => expect(screen.getByTestId("attr-preview")).toBeTruthy());
    expect(screen.getByText("Read-only → on")).toBeTruthy();

    await fireEvent.click(screen.getByTestId("attr-apply"));
    await waitFor(() => expect(screen.getByTestId("attr-undo")).toBeTruthy()); // post-apply state
    const roCalls = calls.filter((c) => c.cmd === "set_readonly" && c.args.readonly === true).map((c) => c.args.path);
    expect(roCalls).toEqual(["C:\\a.txt", "C:\\b.txt"]); // applied to both
  });

  it("Apply is disabled until something changes", async () => {
    render(AttributesDialog, { targets });
    await waitFor(() => expect(screen.getByTestId("attr-apply")).toBeTruthy());
    expect((screen.getByTestId("attr-apply") as HTMLButtonElement).disabled).toBe(true);
  });
});
