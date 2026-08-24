/**
 * Component tests for the browsable Trash view (CPE-1560, epic CPE-1486 final slice): streamed listing,
 * Restore (drops rows on success, surfaces per-item failures), and Empty (confirmed, irreversible) —
 * the GUI counterpart the headless harness can't drive.
 *
 * Mirrors `DiskSpaceView.test.ts`'s mocking strategy: mock `@tauri-apps/api/core`'s `invoke` + `Channel`
 * directly, since `rawInvoke`/`createChannel` (`../invoke`) flow through that module under the local
 * transport, and the generated `commands.*` client's `TAURI_INVOKE` is the same underlying `invoke`.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/svelte";
import { tick } from "svelte";
import TrashView from "./TrashView.svelte";

interface StreamCall {
  cmd: string;
  args: any;
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
}

let streamCalls: StreamCall[] = [];
let invokeImpl: (cmd: string, args?: any) => Promise<any> = () => Promise.reject(new Error("unhandled"));

const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "list_trash_stream") {
    return new Promise((resolve, reject) => streamCalls.push({ cmd, args, resolve, reject }));
  }
  return invokeImpl(cmd, args);
});

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T> {
    onmessage: ((v: T) => void) | null = null;
  }
  return { invoke: (cmd: string, args?: unknown) => invoke(cmd, args), Channel };
});

async function settle() {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  await tick();
}

const entry = (over: Partial<{ id: string; name: string; original_path: string; time_deleted: number; size: number | null }> = {}) => ({
  id: "id-1",
  name: "report.txt",
  original_path: "C:\\Users\\stew\\Documents\\report.txt",
  time_deleted: 1_700_000_000,
  size: 1024,
  ...over,
});

/** Deliver a batch on the most recent `list_trash_stream` call's channel. */
function deliver(batch: ReturnType<typeof entry>[], callIndex = streamCalls.length - 1) {
  streamCalls[callIndex].args.onEntry.onmessage(batch);
}

/** Resolve a `list_trash_stream` call as finished (the real command resolves with a
 *  `{ count, degraded, skipped }` summary once every batch has flushed — a genuinely empty Trash
 *  resolves with `{ count: 0, degraded: false, skipped: 0 }` having sent no batches at all; CPE-1803's
 *  degraded case also sends no batches but resolves with `degraded: true`, which is the whole point —
 *  the two are otherwise indistinguishable. CPE-1804 adds a third shape: batches DID go out and the
 *  summary still reports `degraded: true` with a non-zero `skipped`). */
function finishStream(count: number, degraded = false, skipped = 0, callIndex = streamCalls.length - 1) {
  streamCalls[callIndex].resolve({ count, degraded, skipped });
}

beforeEach(() => {
  invoke.mockClear();
  streamCalls = [];
  invokeImpl = () => Promise.reject(new Error("unhandled"));
});

/** CPE-1827: every toolbar action except Close now lives behind the titlebar's "…" overflow menu (see
 *  TrashView.svelte's titlebar markup) — opens it the same way a user would, by clicking the trigger.
 *  Row checkboxes carry their own `on:click|stopPropagation` (pre-existing, for a different reason —
 *  keeping a checkbox click from double-firing the row's own click-to-select handler), which as a side
 *  effect also keeps them from bubbling to the `<svelte:window on:click>` listener that closes this
 *  menu on an outside click — so selecting rows while the menu is open never closes it out from under a
 *  test (or a real user). */
async function openMenu() {
  await fireEvent.click(screen.getByTitle("More actions"));
  await settle();
}

describe("TrashView — streamed listing (CPE-1560)", () => {
  it("shows a loading state, then paints rows as batches arrive", async () => {
    render(TrashView);
    await settle();
    expect(screen.getByText("Loading Trash…")).toBeTruthy();
    expect(streamCalls).toHaveLength(1);

    deliver([entry()]);
    await settle();

    expect(screen.queryByText("Loading Trash…")).toBeNull();
    expect(screen.getByText("report.txt")).toBeTruthy();
    expect(screen.getByText("C:\\Users\\stew\\Documents\\report.txt")).toBeTruthy();
  });

  it("shows the empty state once the stream finishes having sent no batches", async () => {
    render(TrashView);
    await settle();
    expect(screen.getByText("Loading Trash…")).toBeTruthy();

    // A genuinely empty Trash: the command resolves with count 0, degraded: false, without ever calling
    // onEntry.
    finishStream(0, false);
    await settle();

    expect(screen.queryByText("Loading Trash…")).toBeNull();
    expect(screen.getByText("Trash is empty")).toBeTruthy();
    // CPE-1803: a genuinely empty Trash must NOT also render the degraded message — half of the
    // distinguishability this ticket exists to prove is that these two zero-entry states diverge.
    expect(screen.queryByText(/couldn't be fully read/i)).toBeNull();
  });

  // CPE-1803: a degraded listing (CPE-1791's caught-panic boundary) sends zero batches over the channel,
  // same as a genuinely empty Trash — the ONLY difference the frontend has to go on is the `degraded`
  // flag on the resolved summary. This test and the one above must both pass for the two states to be
  // proven distinguishable; a suite that only asserted this one could pass even if `degraded` were
  // ignored and both cases always rendered "Trash is empty".
  it("shows a distinct degraded state — not 'Trash is empty' — when the listing degraded (CPE-1791/1803)", async () => {
    render(TrashView);
    await settle();
    expect(screen.getByText("Loading Trash…")).toBeTruthy();

    // A degraded pass: same shape as a genuinely empty Trash over the channel (no batches sent), but the
    // resolved summary flags degraded: true. `skipped: 0` — the caught-panic route loses the whole pass
    // rather than n items, so it has no count to report and keeps CPE-1803's unquantified wording.
    finishStream(0, true, 0);
    await settle();

    expect(screen.queryByText("Loading Trash…")).toBeNull();
    expect(screen.getByText("Trash couldn't be fully read — it may not be empty")).toBeTruthy();
    expect(screen.queryByText("Trash is empty")).toBeNull();
    // CPE-1803 review: the titlebar's item count must not sit next to the degraded message asserting a
    // hard "0 items" — that number is exactly what trash.degraded exists to say is NOT known. A stray
    // "0 items" here would contradict the message one line above it.
    expect(screen.queryByText("0 items")).toBeNull();
  });

  // CPE-1816 review round 3, BLOCKING 2: the title bar's own live region goes "Still loading…" -> ""
  // the instant the pass resolves degraded (nothing in its `{#if}` chain matches `complete && degraded`
  // with nothing selected), so without a live region of its OWN, the empty-pane degraded note is
  // announced to nobody — a screen-reader user who heard "Still loading…" a moment ago hears silence,
  // not the actual verdict. This never went through a "Still loading…" phase in THIS specific route (a
  // caught-panic pass sends no batches, so `loading` stays true until resolution), but the same node
  // must still be a live region on its own merits: it's the ONLY place this information is ever stated.
  it("announces the degraded-and-empty note through its own role=status region (CPE-1816)", async () => {
    render(TrashView);
    await settle();
    finishStream(0, true, 0);
    await settle();

    const note = screen.getByText("Trash couldn't be fully read — it may not be empty");
    expect(note.getAttribute("role")).toBe("status");
  });

  // CPE-1804/CPE-1805 — the state this pair exists to prove is degraded-WITH-entries, which before
  // CPE-1804 the backend could not produce: the only degradation route wiped the pass to zero entries,
  // so `entries.length === 0 && degraded` covered every degraded case by accident. Per-item non-UTF-8
  // skips make a partial list the ordinary shape of an incomplete listing, and a partial list rendered
  // as a plain list is the same lie in a new place — the user sees rows and stops looking.
  //
  // These two tests are an asymmetric pair on purpose. The first asserts the notice appears over a list;
  // the second asserts an ordinary complete listing with the SAME rows shows no notice at all. Only the
  // pair is meaningful: a suite with the first alone would pass with the notice rendered unconditionally,
  // which would caveat every healthy Trash forever.
  it("renders the incomplete-listing notice ABOVE the rows when a listing is degraded but has entries (CPE-1805)", async () => {
    render(TrashView);
    await settle();

    // Real batches went out AND the pass was incomplete — the shape only CPE-1804's per-item skip
    // produces, and the one the old `entries.length === 0 && degraded` branch fell straight through.
    deliver([entry({ id: "a", name: "a.txt" }), entry({ id: "b", name: "b.txt" })]);
    await settle();
    finishStream(2, true, 3);
    await settle();

    // The surviving rows are still listed — degrading must not hide what WAS readable.
    expect(screen.getByText("a.txt")).toBeTruthy();
    expect(screen.getByText("b.txt")).toBeTruthy();
    // ...and the notice says so, with the count, rather than the list passing itself off as complete.
    const notice = screen.getByText("3 items in the Trash couldn't be read and aren't shown");
    expect(notice).toBeTruthy();
    // It must sit ABOVE the rows, not below them: a caveat under a list is read after the user has
    // already concluded the list is everything.
    const rowA = screen.getByText("a.txt");
    expect(notice.compareDocumentPosition(rowA) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // Not "Trash is empty" and not the unquantified panic wording — this pass knows the number.
    expect(screen.queryByText("Trash is empty")).toBeNull();
    expect(screen.queryByText("Trash couldn't be fully read — it may not be empty")).toBeNull();
    // CPE-1816 review round 3, BLOCKING 2: the title bar's own live region has nothing to say once the
    // pass resolves degraded (with nothing selected) — this banner is the ONLY place the verdict is
    // announced, so it must carry its own live region rather than relying on the title bar's.
    expect(notice.getAttribute("role")).toBe("status");
  });

  // CPE-1816 review round 2, finding 3: the degraded banner and the column header must occlude each
  // other while the list scrolls only if they compete for the SAME sticky slot — jsdom doesn't compute
  // real sticky/scroll layout, but it does build the real DOM, so this pins the STRUCTURAL fix (both
  // inside one shared sticky container) that prevents that competition, rather than the pixel measurement
  // only a real browser (the Visual Critic's tooling) can take.
  it("wraps the degraded banner and the column header in one shared sticky container (CPE-1816 finding 3)", async () => {
    render(TrashView);
    await settle();
    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();
    finishStream(1, true, 1);
    await settle();

    const notice = screen.getByText("1 item in the Trash couldn't be read and isn't shown");
    const headCheckbox = screen.getByLabelText("Select all");
    const banner = notice.closest(".tv-degraded-banner");
    const headRow = headCheckbox.closest(".tv-head-row");
    expect(banner).toBeTruthy();
    expect(headRow).toBeTruthy();
    // Both must share the SAME sticky ancestor — that's what keeps them stacked in normal flow as one
    // unit instead of each independently sticking to `top: 0` and covering one another.
    const stack = banner?.closest(".tv-sticky-stack");
    expect(stack).toBeTruthy();
    expect(headRow?.closest(".tv-sticky-stack")).toBe(stack);
  });

  // CPE-1816: the gap CPE-1803/CPE-1804/CPE-1805 left open — `degraded`/`skipped` ride on the summary,
  // which by construction resolves LAST, so between the first batch landing and that resolution the view
  // previously had no way to say "this might not be everything yet". This test observes exactly that
  // mid-stream window (batch delivered, summary NOT yet resolved) rather than only the resolved state —
  // a suite that only asserted the final render would pass today and would still pass with the bug intact.
  it("does not assert a finished item count while the stream is still in flight, and says so", async () => {
    render(TrashView);
    await settle();

    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();

    // The row is visible (streaming still paints progressively)...
    expect(screen.getByText("a.txt")).toBeTruthy();
    // ...but the title bar must NOT assert a total this pass can't yet back.
    expect(screen.queryByText("1 item")).toBeNull();
    // A caveat says so, in the title bar's item-count slot (round-2 Visual Critic decision — moved out of
    // a rows-above-the-list banner to avoid a layout jump on the ordinary clean path; see `noticeMessage`
    // in TrashView.svelte for the full reasoning).
    expect(screen.getByText("Still loading…")).toBeTruthy();

    // Once the summary resolves clean, the provisional caveat is replaced by the real, final count.
    finishStream(1, false, 0);
    await settle();
    expect(screen.queryByText("Still loading…")).toBeNull();
    expect(screen.getByText("1 item")).toBeTruthy();
  });

  // CPE-1816 review round 2, a11y finding: the mid-stream caveat and the eventual final count share the
  // SAME element, and that element must announce itself to a screen reader as a live region — otherwise
  // this ticket's fix is visual-only and the underlying "partial list reads as complete" problem persists
  // for anyone who can't see the title bar. Checks the region exists (by its accessible role) and that
  // its announced content actually changes across the mid-stream -> resolved transition — a `role`
  // present on the wrong node, or a region whose content never updates, both defeat the point.
  it("announces the mid-stream caveat and the final count through one role=status region (CPE-1816)", async () => {
    render(TrashView);
    await settle();
    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();

    const region = screen.getByRole("status");
    expect(region.textContent).toContain("Still loading…");

    finishStream(1, false, 0);
    await settle();

    // Same node (one persistent live region, not a fresh one per state) — its CONTENT is what changed.
    expect(screen.getByRole("status")).toBe(region);
    expect(region.textContent).toContain("1 item");
    expect(region.textContent).not.toContain("Still loading…");
  });

  // CPE-1816 review round 3 (nit, promoted to a real assertion): Svelte trims the leading whitespace
  // immediately inside an `{#if}` block's static text, so a naive `{#if selected.size > 0} ·
  // {selectedCountLabel}{/if}` silently lost its leading space and read as "...· 1 selected" with no gap
  // — audible to a screen reader reading this exact live region, not just a cosmetic gap. Covers BOTH
  // places the separator appears: the pre-existing resolved-count branch (where the bug already lived)
  // and the new mid-stream branch this ticket added (which would otherwise have duplicated it).
  it("keeps a real space before the selection-count separator, mid-stream and resolved alike (CPE-1816 nit)", async () => {
    render(TrashView);
    await settle();
    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();
    await fireEvent.click(screen.getByLabelText("a.txt"));

    // Mid-stream + selected.
    expect(screen.getByRole("status").textContent).toBe("Still loading… · 1 selected");

    finishStream(1, false, 0);
    await settle();

    // Resolved + selected — the pre-existing branch, fixed the same way rather than left to drift.
    expect(screen.getByRole("status").textContent).toBe("1 item · 1 selected");
  });

  // CPE-1816 review round 2, BLOCKER 1: `complete` must be reset on every `load()` call, including a
  // Refresh — otherwise a Refresh's brand-new, still-in-flight stream inherits the PREVIOUS pass's
  // resolved `true`, and this exact bug reappears on the second load. Probe shape from the Reviewer:
  // open, deliver, finish, Refresh, deliver again on the NEW stream without finishing it, and check that
  // the stale resolved state doesn't leak through.
  it("resets the in-flight caveat on Refresh, rather than keeping a stale resolved state from the previous pass (CPE-1816)", async () => {
    render(TrashView);
    await settle();
    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();
    finishStream(1, false, 0);
    await settle();
    expect(screen.getByText("1 item")).toBeTruthy();

    await openMenu();
    await fireEvent.click(screen.getByText("Refresh"));
    await settle();
    // The new stream (call index 1) delivers a batch but is deliberately left unresolved.
    deliver([entry({ id: "b", name: "b.txt" })], 1);
    await settle();

    // The new pass hasn't resolved yet — it must not inherit the previous pass's "1 item" verdict.
    expect(screen.queryByText("1 item")).toBeNull();
    expect(screen.getByText("Still loading…")).toBeTruthy();
  });

  it("renders NO notice for an ordinary complete listing with the same rows (CPE-1805 asymmetry)", async () => {
    render(TrashView);
    await settle();

    deliver([entry({ id: "a", name: "a.txt" }), entry({ id: "b", name: "b.txt" })]);
    await settle();
    finishStream(2, false, 0);
    await settle();

    expect(screen.getByText("a.txt")).toBeTruthy();
    expect(screen.getByText("b.txt")).toBeTruthy();
    expect(screen.queryByText(/couldn't be read and aren't shown/i)).toBeNull();
    expect(screen.queryByText(/couldn't be fully read/i)).toBeNull();
    // The count is only suppressed for an incomplete pass — a healthy one still reports it.
    expect(screen.getByText("2 items")).toBeTruthy();
  });

  it("uses the singular wording when exactly one item was skipped (CPE-1804)", async () => {
    render(TrashView);
    await settle();
    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();
    finishStream(1, true, 1);
    await settle();

    expect(screen.getByText("1 item in the Trash couldn't be read and isn't shown")).toBeTruthy();
    expect(screen.queryByText(/^1 items /)).toBeNull();
  });

  // CPE-1804's own harm, in its purest form: a Trash holding ONLY undecodable items. Zero rows arrive,
  // exactly like a genuinely empty Trash, and before this fix the view said "Trash is empty" — the
  // report that makes someone stop looking for a file that is still there.
  it("an all-skipped listing reports the skipped count, never 'Trash is empty' (CPE-1804)", async () => {
    render(TrashView);
    await settle();
    finishStream(0, true, 4);
    await settle();

    expect(screen.getByText("4 items in the Trash couldn't be read and aren't shown")).toBeTruthy();
    expect(screen.queryByText("Trash is empty")).toBeNull();
    expect(screen.queryByText("0 items")).toBeNull();
  });

  it("clears a previous pass's skipped count on refresh (CPE-1804)", async () => {
    render(TrashView);
    await settle();
    finishStream(0, true, 4);
    await settle();
    expect(screen.getByText("4 items in the Trash couldn't be read and aren't shown")).toBeTruthy();

    await openMenu();
    await fireEvent.click(screen.getByText("Refresh"));
    await settle();
    finishStream(0, false, 0, 1);
    await settle();

    // A healed Trash must not keep asserting a stale skip count from the pass before it.
    expect(screen.queryByText(/couldn't be read and aren't shown/i)).toBeNull();
    expect(screen.getByText("Trash is empty")).toBeTruthy();
  });

  it("appends multiple batches across the same stream", async () => {
    render(TrashView);
    await settle();
    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();
    deliver([entry({ id: "b", name: "b.txt" })]);
    await settle();

    expect(screen.getByText("a.txt")).toBeTruthy();
    expect(screen.getByText("b.txt")).toBeTruthy();
  });

  it("Refresh re-invokes the stream and supersedes a stale in-flight one", async () => {
    render(TrashView);
    await settle();
    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();
    expect(screen.getByText("a.txt")).toBeTruthy();

    await openMenu();
    await fireEvent.click(screen.getByText("Refresh"));
    await settle();
    expect(streamCalls).toHaveLength(2);
    // The list is cleared immediately on refresh, even before the new stream sends anything.
    expect(screen.queryByText("a.txt")).toBeNull();

    // A stale batch from the FIRST (superseded) stream must be dropped.
    deliver([entry({ id: "stale", name: "stale.txt" })], 0);
    await settle();
    expect(screen.queryByText("stale.txt")).toBeNull();

    deliver([entry({ id: "b", name: "b.txt" })], 1);
    await settle();
    expect(screen.getByText("b.txt")).toBeTruthy();
  });

  it("shows a friendly error when the stream invoke rejects", async () => {
    invoke.mockImplementationOnce((cmd: string) => {
      if (cmd === "list_trash_stream") return Promise.reject("boom");
      return Promise.reject(new Error("unhandled"));
    });
    render(TrashView);
    await settle();
    expect(screen.getByText(/Couldn't load the Trash/)).toBeTruthy();
  });
});

describe("TrashView — Restore (CPE-1560)", () => {
  // CPE-1816 review round 2: resolves the stream (a clean, complete pass) before returning, rather than
  // leaving it perpetually in flight as before. Restore/Empty are ordinarily clicked against a FINISHED
  // listing, and leaving the stream unresolved here was itself masking a bug — see
  // "does not claim 'Trash is empty' when Restore drains ... mid-stream" below, which exercises the
  // in-flight case deliberately and explicitly instead.
  async function renderWithTwoEntries() {
    const utils = render(TrashView);
    await settle();
    deliver([
      entry({ id: "keep", name: "keep.txt" }),
      entry({ id: "gone", name: "gone.txt" }),
    ]);
    await settle();
    finishStream(2, false, 0);
    await settle();
    return utils;
  }

  it("Restore selected is disabled until something is checked", async () => {
    await renderWithTwoEntries();
    await openMenu();
    const restoreBtn = screen.getByText("Restore selected").closest("button") as HTMLButtonElement;
    expect(restoreBtn.disabled).toBe(true);

    await fireEvent.click(screen.getByLabelText("gone.txt"));
    expect(restoreBtn.disabled).toBe(false);
  });

  it("restores the checked item, drops it from the list, and calls the command with its id", async () => {
    invokeImpl = (cmd) => {
      if (cmd === "restore_trash_items") {
        return Promise.resolve([{ path: "C:\\restored\\gone.txt", ok: true, error: "" }]);
      }
      return Promise.reject(new Error(`unexpected ${cmd}`));
    };
    await renderWithTwoEntries();

    await fireEvent.click(screen.getByLabelText("gone.txt"));
    await openMenu();
    await fireEvent.click(screen.getByText("Restore selected"));
    await settle();

    expect(invoke).toHaveBeenCalledWith("restore_trash_items", { ids: ["gone"] });
    expect(screen.queryByText("gone.txt")).toBeNull();
    expect(screen.getByText("keep.txt")).toBeTruthy();
  });

  it("surfaces a per-item restore failure without removing the row or the rest of the selection", async () => {
    invokeImpl = (cmd) => {
      if (cmd === "restore_trash_items") {
        return Promise.resolve([
          { path: "C:\\restored\\keep.txt", ok: true, error: "" },
          { path: "gone", ok: false, error: "Something already exists at the original location" },
        ]);
      }
      return Promise.reject(new Error(`unexpected ${cmd}`));
    };
    await renderWithTwoEntries();

    await fireEvent.click(screen.getByLabelText("keep.txt"));
    await fireEvent.click(screen.getByLabelText("gone.txt"));
    await openMenu();
    await fireEvent.click(screen.getByText("Restore selected"));
    await settle();

    // The failed item stays in the list; the successfully restored one is dropped.
    expect(screen.queryByText("keep.txt")).toBeNull();
    expect(screen.getByText("gone.txt")).toBeTruthy();
    expect(screen.getByText(/Couldn't restore gone\.txt/)).toBeTruthy();
    expect(screen.getByText(/Something already exists/)).toBeTruthy();
  });

  // CPE-1816 review round 2, BLOCKER 2: the Reviewer demonstrated this is reachable, not theoretical —
  // deliver a batch, leave the stream unresolved (a large/slow trash is exactly the case this ticket is
  // about), select the visible row and restore it. Draining `entries` to zero must NOT fall through to
  // "Trash is empty": the pass hasn't finished, so more rows could still be coming.
  it("does not claim 'Trash is empty' when Restore drains the last visible row while the stream is still in flight (CPE-1816)", async () => {
    invokeImpl = (cmd) => {
      if (cmd === "restore_trash_items") {
        return Promise.resolve([{ path: "C:\\restored\\a.txt", ok: true, error: "" }]);
      }
      return Promise.reject(new Error(`unexpected ${cmd}`));
    };
    render(TrashView);
    await settle();
    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();
    // Deliberately no `finishStream` — the pass is still in flight when the user acts.

    await fireEvent.click(screen.getByLabelText("a.txt"));
    await openMenu();
    await fireEvent.click(screen.getByText("Restore selected"));
    await settle();

    expect(screen.queryByText("Trash is empty")).toBeNull();
    expect(screen.getByText("Still loading…")).toBeTruthy();
  });
});

describe("TrashView — Empty (CPE-1560, irreversible → ConfirmDialog per MENUS.md)", () => {
  // CPE-1816 review round 2: resolves the stream before returning — see the matching note in the Restore
  // describe block above. The in-flight case is exercised deliberately below instead.
  async function renderWithTwoEntries() {
    const utils = render(TrashView);
    await settle();
    deliver([
      entry({ id: "a", name: "a.txt" }),
      entry({ id: "b", name: "b.txt" }),
    ]);
    await settle();
    finishStream(2, false, 0);
    await settle();
    return utils;
  }

  it("Empty Trash opens a confirm dialog before purging anything", async () => {
    invokeImpl = () => Promise.reject(new Error("empty_trash must not be called before confirming"));
    await renderWithTwoEntries();

    await openMenu();
    await fireEvent.click(screen.getByText("Empty Trash"));
    await settle();

    expect(screen.getByText("Empty Trash?")).toBeTruthy();
    expect(invoke).not.toHaveBeenCalledWith("empty_trash", expect.anything());
  });

  it("confirming Empty Trash purges everything and calls empty_trash with no ids", async () => {
    invokeImpl = (cmd) => (cmd === "empty_trash" ? Promise.resolve(null) : Promise.reject(new Error(cmd)));
    await renderWithTwoEntries();

    await openMenu();
    await fireEvent.click(screen.getByText("Empty Trash"));
    await settle();
    // The dialog's own primary button carries the same confirm label.
    const dialog = screen.getByText("Empty Trash?").closest(".dialog") as HTMLElement;
    await fireEvent.click(within(dialog).getByText("Empty Trash"));
    await settle();

    // `confirmed: true` (CPE-1651) — the backend refuses an unconsented purge, and this dialog is the
    // only thing allowed to set it.
    expect(invoke).toHaveBeenCalledWith("empty_trash", { ids: null, confirmed: true });
    expect(screen.getByText("Trash is empty")).toBeTruthy();
  });

  it("cancelling the confirm dialog leaves the Trash untouched", async () => {
    invokeImpl = () => Promise.reject(new Error("empty_trash must not be called"));
    await renderWithTwoEntries();

    await openMenu();
    await fireEvent.click(screen.getByText("Empty Trash"));
    await settle();
    await fireEvent.click(screen.getByText("Cancel"));
    await settle();

    expect(screen.queryByText("Empty Trash?")).toBeNull();
    expect(screen.getByText("a.txt")).toBeTruthy();
    expect(screen.getByText("b.txt")).toBeTruthy();
  });

  it("Delete selected permanently confirms with the selected count, then purges only those ids", async () => {
    invokeImpl = (cmd) => (cmd === "empty_trash" ? Promise.resolve(null) : Promise.reject(new Error(cmd)));
    await renderWithTwoEntries();

    await fireEvent.click(screen.getByLabelText("a.txt"));
    await openMenu();
    await fireEvent.click(screen.getByText("Delete selected permanently"));
    await settle();

    expect(screen.getByText(/permanently deletes 1 selected item/)).toBeTruthy();
    const dialog = screen.getByText("Empty Trash?").closest(".dialog") as HTMLElement;
    await fireEvent.click(within(dialog).getByText("Empty Trash"));
    await settle();

    expect(invoke).toHaveBeenCalledWith("empty_trash", { ids: ["a"], confirmed: true });
    expect(screen.queryByText("a.txt")).toBeNull();
    expect(screen.getByText("b.txt")).toBeTruthy();
  });

  it("Delete selected permanently is disabled with nothing checked", async () => {
    await renderWithTwoEntries();
    await openMenu();
    const btn = screen.getByText("Delete selected permanently").closest("button") as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  // CPE-1816 review round 2, BLOCKER 2: same shape as the Restore version above, via Empty Trash instead
  // — `confirmEmptyAction`'s "all" branch also clears `entries` without touching `complete`.
  it("does not claim 'Trash is empty' when Empty Trash drains the last visible row while the stream is still in flight (CPE-1816)", async () => {
    invokeImpl = (cmd) => (cmd === "empty_trash" ? Promise.resolve(null) : Promise.reject(new Error(cmd)));
    render(TrashView);
    await settle();
    deliver([entry({ id: "a", name: "a.txt" })]);
    await settle();
    // Deliberately no `finishStream` — the pass is still in flight when the user acts.

    await openMenu();
    await fireEvent.click(screen.getByText("Empty Trash"));
    await settle();
    const dialog = screen.getByText("Empty Trash?").closest(".dialog") as HTMLElement;
    await fireEvent.click(within(dialog).getByText("Empty Trash"));
    await settle();

    expect(screen.queryByText("Trash is empty")).toBeNull();
    expect(screen.getByText("Still loading…")).toBeTruthy();
  });
});

describe("TrashView — chrome", () => {
  it("dispatches close from the × button", async () => {
    const { component } = render(TrashView);
    await settle();
    const closed = vi.fn();
    component.$on("close", closed);

    await fireEvent.click(screen.getByTitle("Close"));
    expect(closed).toHaveBeenCalledOnce();
  });
});
