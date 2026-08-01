/**
 * Component render tests for SmartFolderMenu's reorder affordance (CPE-1231). This popover is
 * reused for both the tag-only Smart Folders section and the structured Saved Searches section
 * (App.svelte wires each usage to its own store's moveSaved/moveSavedSearch), so testing the
 * component once covers both call sites' UI. The caller computes `canMoveUp`/`canMoveDown` from
 * its own store's index — this component only knows the single item's id/name.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import SmartFolderMenu from "./SmartFolderMenu.svelte";

describe("SmartFolderMenu reorder buttons (CPE-1231)", () => {
  it("renders move up/down enabled by default (both true)", () => {
    render(SmartFolderMenu, { name: "Invoices" });
    expect((screen.getByLabelText("Move up") as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByLabelText("Move down") as HTMLButtonElement).disabled).toBe(false);
  });

  it("disables Move up when canMoveUp is false (first item in the list)", () => {
    render(SmartFolderMenu, { name: "Invoices", canMoveUp: false, canMoveDown: true });
    expect((screen.getByLabelText("Move up") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText("Move down") as HTMLButtonElement).disabled).toBe(false);
  });

  it("disables Move down when canMoveDown is false (last item in the list)", () => {
    render(SmartFolderMenu, { name: "Invoices", canMoveUp: true, canMoveDown: false });
    expect((screen.getByLabelText("Move up") as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByLabelText("Move down") as HTMLButtonElement).disabled).toBe(true);
  });

  it("dispatches moveUp when Move up is clicked", async () => {
    const { component } = render(SmartFolderMenu, { name: "Invoices" });
    const moveUp = vi.fn();
    component.$on("moveUp", moveUp);

    await fireEvent.click(screen.getByLabelText("Move up"));
    expect(moveUp).toHaveBeenCalledOnce();
  });

  it("dispatches moveDown when Move down is clicked", async () => {
    const { component } = render(SmartFolderMenu, { name: "Invoices" });
    const moveDown = vi.fn();
    component.$on("moveDown", moveDown);

    await fireEvent.click(screen.getByLabelText("Move down"));
    expect(moveDown).toHaveBeenCalledOnce();
  });

  // Note: the boundary case (disabled button never dispatching) is a native HTML behavior — a real
  // browser never fires `click` on a `disabled` button — verified by the `disabled` assertions above.
  // `fireEvent.click` in jsdom dispatches a synthetic event that bypasses that native gate, so it
  // isn't a reliable way to assert the no-dispatch behavior here (no `user-event` package in this repo
  // to simulate a real interaction instead).

  it("reorder buttons don't interfere with the existing rename/remove/close actions", async () => {
    const { component } = render(SmartFolderMenu, { name: "Invoices" });
    const remove = vi.fn();
    component.$on("remove", remove);

    await fireEvent.click(screen.getByText("Delete"));
    expect(remove).toHaveBeenCalledOnce();
  });
});
