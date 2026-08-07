/**
 * Component render tests for the sidecar platform management panel (CPE-274/863, ticket CPE-1406).
 *
 * SidecarManager has no direct `invoke`/`@tauri-apps/api/core` calls of its own — it goes through the
 * typed `../sidecar` client (`sidecarDetails`, `setEnabled`, `stopSidecar`, `revokeCapability`,
 * `setConsent`, `repairSidecar`, `sidecarDiagnostics`), so this spec mocks that module directly and
 * exercises two things the component does with real logic:
 *
 *  1. `statusOf` — a worst-first, 5-way priority derivation of the health pill: missing binary beats
 *     incompatible contract beats disabled beats a stalled last-error beats running beats the plain
 *     "ready" idle state. Each priority test stacks *every* worse-or-equal condition onto the fixture
 *     to prove the earlier check actually wins, not just that each condition renders correctly in
 *     isolation.
 *  2. The 5 action wirings (toggle/stop/revoke/grant/repair) — each must call its sidecar-client
 *     function with the exact args implied by the row under test, and each must re-`refresh()`
 *     (re-fetch `sidecarDetails` + `sidecarDiagnostics`) afterward so the panel reflects the new state.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";

const {
  sidecarDetails,
  setEnabled,
  stopSidecar,
  revokeCapability,
  setConsent,
  repairSidecar,
  sidecarDiagnostics,
} = vi.hoisted(() => ({
  sidecarDetails: vi.fn(),
  setEnabled: vi.fn(),
  stopSidecar: vi.fn(),
  revokeCapability: vi.fn(),
  setConsent: vi.fn(),
  repairSidecar: vi.fn(),
  sidecarDiagnostics: vi.fn(),
}));

// Mock only the I/O-performing exports; keep the real `CAPABILITY_INFO` labels + types so the
// rendered capability pills use their actual copy.
vi.mock("../sidecar", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../sidecar")>();
  return {
    ...actual,
    sidecarDetails,
    setEnabled,
    stopSidecar,
    revokeCapability,
    setConsent,
    repairSidecar,
    sidecarDiagnostics,
  };
});

import SidecarManager from "./SidecarManager.svelte";
import type { SidecarInfo, SidecarDiagnostics } from "../sidecar";

// ── fixtures ──────────────────────────────────────────────────────────────────────────────────

function makeRow(over: Partial<SidecarInfo> = {}): SidecarInfo {
  return {
    id: "ai-console",
    name: "AI Console",
    version: "1.0.0",
    contract: "1",
    compatible: true,
    running: false,
    enabled: true,
    binary_ok: true,
    requested: [],
    granted: [],
    ...over,
  };
}

function makeDiag(over: Partial<SidecarDiagnostics> = {}): SidecarDiagnostics {
  return { id: "ai-console", running: false, last_error: null, logs: [], ...over };
}

/** Renders the panel with a single row + matching diagnostics, waiting for the first paint (the
 *  component fetches both asynchronously in `refresh()` on mount). */
async function renderRow(rowOver: Partial<SidecarInfo> = {}, diagOver: Partial<SidecarDiagnostics> = {}) {
  const row = makeRow(rowOver);
  const diag = makeDiag({ id: row.id, ...diagOver });
  sidecarDetails.mockResolvedValue([row]);
  sidecarDiagnostics.mockResolvedValue(diag);
  const utils = render(SidecarManager);
  await waitFor(() => expect(screen.getByText(row.name)).toBeTruthy());
  return { row, diag, ...utils };
}

beforeEach(() => {
  sidecarDetails.mockReset();
  setEnabled.mockReset();
  stopSidecar.mockReset();
  revokeCapability.mockReset();
  setConsent.mockReset();
  repairSidecar.mockReset();
  sidecarDiagnostics.mockReset();
  setEnabled.mockResolvedValue(true);
  stopSidecar.mockResolvedValue(true);
  revokeCapability.mockResolvedValue(true);
  setConsent.mockResolvedValue(true);
  repairSidecar.mockResolvedValue({ id: "ai-console", binary_ok: true, actions: ["reaped 1 orphan"] });
});

// ── loading / empty states ───────────────────────────────────────────────────────────────────

describe("SidecarManager — loading/empty states", () => {
  it("shows the checking placeholder before sidecarDetails resolves", () => {
    sidecarDetails.mockReturnValue(new Promise(() => {})); // never resolves
    sidecarDiagnostics.mockResolvedValue(makeDiag());
    render(SidecarManager);
    expect(screen.getByText("Checking…")).toBeTruthy();
  });

  it("shows the none-registered message when sidecarDetails resolves empty", async () => {
    sidecarDetails.mockResolvedValue([]);
    render(SidecarManager);
    await waitFor(() => expect(screen.getByText("No sidecars registered.")).toBeTruthy());
  });
});

// ── statusOf: worst-first 5-way priority ─────────────────────────────────────────────────────

describe("SidecarManager — statusOf worst-first priority (CPE-863)", () => {
  it("binary_ok:false wins over everything else (incompatible + disabled + errored + running all true/set)", async () => {
    const { container } = await renderRow(
      { binary_ok: false, compatible: false, enabled: false, running: true },
      { last_error: "boom" },
    );
    const pill = container.querySelector(".status") as HTMLElement;
    expect(pill.textContent).toBe("Missing");
    expect(pill.classList.contains("bad")).toBe(true);
  });

  it("compatible:false wins over disabled + errored + running, when binary_ok is true", async () => {
    const { container } = await renderRow(
      { binary_ok: true, compatible: false, enabled: false, running: true },
      { last_error: "boom" },
    );
    const pill = container.querySelector(".status") as HTMLElement;
    expect(pill.textContent).toBe("Incompatible");
    expect(pill.classList.contains("bad")).toBe(true);
  });

  it("enabled:false wins over errored + running, when binary_ok + compatible are true", async () => {
    const { container } = await renderRow(
      { binary_ok: true, compatible: true, enabled: false, running: true },
      { last_error: "boom" },
    );
    const pill = container.querySelector(".status") as HTMLElement;
    expect(pill.textContent).toBe("Disabled");
    expect(pill.classList.contains("idle")).toBe(true);
  });

  it("a stalled last_error (not running) wins over the plain ready state, when binary_ok + compatible + enabled are true", async () => {
    const { container } = await renderRow(
      { binary_ok: true, compatible: true, enabled: true, running: false },
      { last_error: "boom" },
    );
    const pill = container.querySelector(".status") as HTMLElement;
    expect(pill.textContent).toBe("Error");
    expect(pill.classList.contains("warn")).toBe(true);
  });

  it("running:true beats a last_error that's still set (the error check requires NOT running)", async () => {
    const { container } = await renderRow(
      { binary_ok: true, compatible: true, enabled: true, running: true },
      { last_error: "boom" },
    );
    const pill = container.querySelector(".status") as HTMLElement;
    expect(pill.textContent).toBe("Running");
    expect(pill.classList.contains("ok")).toBe(true);
  });

  it("falls through to the idle 'ready' state when nothing else applies", async () => {
    const { container } = await renderRow(
      { binary_ok: true, compatible: true, enabled: true, running: false },
      { last_error: null },
    );
    const pill = container.querySelector(".status") as HTMLElement;
    expect(pill.textContent).toBe("Ready");
    expect(pill.classList.contains("idle")).toBe(true);
  });
});

// ── the 5 action wirings ─────────────────────────────────────────────────────────────────────

describe("SidecarManager — action wirings (each calls its command + re-refreshes)", () => {
  it("toggle: clicking the enable switch calls setEnabled(id, !enabled) and refreshes", async () => {
    const { container, row } = await renderRow({ enabled: true });
    sidecarDetails.mockClear();
    sidecarDiagnostics.mockClear();

    const checkbox = container.querySelector('.switch input[type="checkbox"]') as HTMLInputElement;
    await fireEvent.click(checkbox);

    expect(setEnabled).toHaveBeenCalledTimes(1);
    expect(setEnabled).toHaveBeenCalledWith(row.id, false);
    await waitFor(() => expect(sidecarDetails).toHaveBeenCalledTimes(1)); // refresh() re-fetched
    await waitFor(() => expect(sidecarDiagnostics).toHaveBeenCalledWith(row.id));
  });

  it("toggle: on a disabled row, clicking the switch calls setEnabled(id, true)", async () => {
    const { row, container } = await renderRow({ enabled: false });
    // Both the status pill and the switch label render "Disabled" text (same $t key), so scope to the
    // switch's own checkbox rather than a text query.
    const checkbox = container.querySelector('.switch input[type="checkbox"]') as HTMLInputElement;
    await fireEvent.click(checkbox);
    expect(setEnabled).toHaveBeenCalledWith(row.id, true);
  });

  it("stop: clicking Stop (only shown for ai-console while running) calls stopSidecar(id) and refreshes", async () => {
    const { row } = await renderRow({ id: "ai-console", running: true, enabled: true });
    sidecarDetails.mockClear();

    await fireEvent.click(screen.getByText("Stop"));

    expect(stopSidecar).toHaveBeenCalledTimes(1);
    expect(stopSidecar).toHaveBeenCalledWith(row.id);
    await waitFor(() => expect(sidecarDetails).toHaveBeenCalledTimes(1));
  });

  it("revoke: clicking × on a granted capability calls revokeCapability(id, cap) and refreshes", async () => {
    const { row } = await renderRow({ requested: ["context"], granted: ["context"] });
    sidecarDetails.mockClear();

    await fireEvent.click(screen.getByTitle("Revoke"));

    expect(revokeCapability).toHaveBeenCalledTimes(1);
    expect(revokeCapability).toHaveBeenCalledWith(row.id, "context");
    await waitFor(() => expect(sidecarDetails).toHaveBeenCalledTimes(1));
  });

  it("grant: clicking Grant on an undecided capability calls setConsent(id, granted-with-cap, same-list) and refreshes", async () => {
    const { row } = await renderRow({ requested: ["secrets"], granted: [] });
    sidecarDetails.mockClear();

    await fireEvent.click(screen.getByText("Grant"));

    expect(setConsent).toHaveBeenCalledTimes(1);
    expect(setConsent).toHaveBeenCalledWith(row.id, ["secrets"], ["secrets"]);
    await waitFor(() => expect(sidecarDetails).toHaveBeenCalledTimes(1));
  });

  it("grant: appends the new capability onto any already-granted ones rather than replacing them", async () => {
    const { row } = await renderRow({ requested: ["context", "network"], granted: ["context"] });

    await fireEvent.click(screen.getByText("Grant")); // the only ungranted requested cap is "network"

    expect(setConsent).toHaveBeenCalledWith(row.id, ["context", "network"], ["context", "network"]);
  });

  it("repair: clicking Repair calls repairSidecar(id), refreshes, and shows the joined actions", async () => {
    const { row } = await renderRow();
    sidecarDetails.mockClear();
    repairSidecar.mockResolvedValue({ id: row.id, binary_ok: true, actions: ["reaped 1 orphan", "cleared last error"] });

    await fireEvent.click(screen.getByText("Repair"));

    expect(repairSidecar).toHaveBeenCalledTimes(1);
    expect(repairSidecar).toHaveBeenCalledWith(row.id);
    await waitFor(() => expect(sidecarDetails).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toBe("Repaired: reaped 1 orphan; cleared last error"),
    );
  });

  it("repair: a null result (platform off / repair failed) still refreshes and surfaces the failure copy", async () => {
    const { row } = await renderRow();
    sidecarDetails.mockClear();
    repairSidecar.mockResolvedValue(null);

    await fireEvent.click(screen.getByText("Repair"));

    expect(repairSidecar).toHaveBeenCalledWith(row.id);
    await waitFor(() => expect(sidecarDetails).toHaveBeenCalledTimes(1));
    // Fixed (CPE-1408): a failed repair must NOT be prefixed "Repaired: " — that reads as success.
    // The status line shows the failure copy alone, with a warn-toned ".fail" class.
    const status = screen.getByRole("status");
    await waitFor(() => expect(status.textContent).toBe("Repair failed — the platform may be off"));
    expect(status.classList.contains("fail")).toBe(true);
  });
});
