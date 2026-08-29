/**
 * MacroRunConfirm (CPE-1191, epic CPE-739): the dry-run (macro_plan) -> confirm -> execute
 * (macro_run) -> offer Undo (macro_undo) safety gate for running a saved macro. The typed
 * `commands.*` client routes through the mocked `../invoke`, mirroring TemplatesDialog.test.ts.
 *
 * CPE-1891 added `macro_preflight` (a read-only real-filesystem collision scan run alongside
 * `macro_plan`) and threaded `confirmedOverwrite` through `macro_run` as the list of destinations the
 * user actually confirmed (PR #1044 review round 2, Blocker 2 -- never a blanket bool), so a colliding
 * destination no longer aborts and rolls back the whole batch with no recourse — the tests below cover
 * that flow, plus the review round's reason-rendering redesign (one sentence per hazard KIND, hoisted
 * above the path list, not one per row) and its irreversibility warning.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { stripRustComments, rustStringLiteralAfter } from "../rustSource";
import { styleBlock, declaration, contentIndependentHeightReason } from "../svelteCss";

// The two hazard sentences EXACTLY as the box must render them (CPE-1928): differentiator first,
// path stripped (CPE-1891, Visual Critic round 3 — the hoisted sentence is shared across
// possibly-many collisions and must not look tied to just the first one's path), and the remedy both
// kinds share lifted out to `SHARED_REMEDY` so it is stated once. Pinned as literals rather than
// re-derived from the fixture by a mirrored transform, so a copy change has to be made here too.
const RENAME_HAZARD_SENTENCE =
  "Renaming onto a link destroys it — the link is removed and its target is left orphaned.";
const CONVERT_HAZARD_SENTENCE =
  "Creating a file at a link's name writes THROUGH it — the bytes would land at the link's target, " +
  "a path you did not name, and a failure part-way would then delete the link itself.";
const SHARED_REMEDY = "Nothing was changed; remove the link first if that is what you meant.";

const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null);
vi.mock("../invoke", () => ({
  invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a),
  unwrap: <T>(r: { status: string; data?: T; error?: unknown }): T => {
    if (r.status === "ok") return r.data as T;
    throw r.error instanceof Error ? r.error : new Error(String(r.error));
  },
}));

import MacroRunConfirm from "./MacroRunConfirm.svelte";

const MACRO = { name: "Tidy", steps: [{ rename: { template: "{stem}_v2.{ext}" } }] };
const PLAN = [{ input: "/work/a.txt", kind: "rename", detail: "/work/a_v2.txt" }];
const RESOLVED_RUN = {
  ops: [{ from: "/work/a.txt", kind: "rename", detail: "a_v2.txt", to: "/work/a_v2.txt" }],
  inverses: [{ from: "/work/a_v2.txt", kind: "rename", detail: "a.txt", to: "/work/a.txt" }],
};
const CONFIRMABLE_COLLISION = {
  op_index: 0,
  from: "/work/a.txt",
  to: "/work/a_v2.txt",
  kind: "rename",
  confirmable: true,
  reason: '"a_v2.txt" already exists',
};
const BLOCKED_COLLISION = {
  op_index: 0,
  from: "/work/a.txt",
  to: "/work/a_v2.txt",
  kind: "rename",
  confirmable: false,
  // Verbatim `classify_symlink_slot` (crates/server/src/fsutil.rs), remedy tail included -- the tail is
  // exactly what CPE-1928 hoists out, so a fixture clipped short of it could not exercise the hoist.
  reason:
    '"/work/a_v2.txt" is a link, and renaming onto a link destroys it — the link is removed and its ' +
    "target is left orphaned. Nothing was changed; remove the link first if that is what you meant",
};
// Same shape, the CONVERT-flavored wording (CPE-1891 follow-up: the two must read differently, not just
// share a generic "is a link" header).
const CONVERT_BLOCKED_COLLISION = {
  op_index: 1,
  from: "/work/b.png",
  to: "/work/b.jpg",
  kind: "convert",
  confirmable: false,
  // Verbatim `classify_create_slot` -- note its remedy tail says "Nothing was WRITTEN", the rename
  // guard's says "Nothing was CHANGED"; both must reduce to the one shared remedy line.
  reason:
    '"/work/b.jpg" is a link, and creating a file at a link\'s name writes THROUGH it — the bytes ' +
    "would land at the link's target, a path you did not name, and a failure part-way would then " +
    "delete the link itself. Nothing was written; remove the link first if that is what you meant",
};
// A THIRD blocked item, same "rename-move" bucket as BLOCKED_COLLISION but a DIFFERENT path/reason
// string -- proves the dedup is by hazard kind, not by exact reason text.
const MOVE_BLOCKED_COLLISION = {
  op_index: 2,
  from: "/work/c.txt",
  to: "/work/Archive/c.txt",
  kind: "move",
  confirmable: false,
  reason:
    '"/work/Archive/c.txt" is a link, and renaming onto a link destroys it — the link is removed and ' +
    "its target is left orphaned. Nothing was changed; remove the link first if that is what you meant",
};

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "macro_plan") return PLAN;
    if (cmd === "macro_preflight") return [];
    return null;
  });
});

describe("MacroRunConfirm dry-run + confirm (CPE-1191)", () => {
  it("dry-runs via macro_plan on mount and renders the planned ops", async () => {
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("plan-list");
    expect(invokeMock).toHaveBeenCalledWith("macro_plan", { macro: MACRO, inputs: ["/work/a.txt"] });
    expect(screen.getByTestId("plan-list").textContent).toContain("/work/a.txt");
  });

  it("also preflights via macro_preflight on mount, with macro/inputs/root", async () => {
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("plan-list");
    expect(invokeMock).toHaveBeenCalledWith("macro_preflight", {
      macro: MACRO,
      inputs: ["/work/a.txt"],
      root: "/work",
    });
  });

  it("Run is disabled while the plan is still loading, and calls macro_run with an EMPTY confirmedOverwrite when nothing collided", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_run") return RESOLVED_RUN;
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [];
      return null;
    });

    const { component } = render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("plan-list");
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(false);

    const ran = vi.fn();
    component.$on("ran", (e: CustomEvent) => ran(e.detail));

    await fireEvent.click(screen.getByTestId("run-btn"));

    expect(invokeMock).toHaveBeenCalledWith("macro_run", {
      macro: MACRO,
      inputs: ["/work/a.txt"],
      root: "/work",
      confirmedOverwrite: [],
    });
    expect(ran).toHaveBeenCalledWith(RESOLVED_RUN);
  });

  it("shows an empty-plan message and disables Run when there's nothing to do", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return [];
      if (cmd === "macro_preflight") return [];
      return null;
    });
    render(MacroRunConfirm, { macro: MACRO, inputs: [], root: "/work" });
    await screen.findByTestId("plan-list");
    expect(screen.getByTestId("plan-list").textContent).toContain("Nothing to run");
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(true);
  });

  it("a macro_plan failure shows the error and keeps Run disabled", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") throw new Error("boom");
      if (cmd === "macro_preflight") return [];
      return null;
    });
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    expect(await screen.findByTestId("plan-error")).toHaveProperty("textContent", "boom");
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(true);
  });

  it("a macro_preflight failure shows its own error without blocking the plan preview", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") throw new Error("preflight boom");
      return null;
    });
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    expect(await screen.findByTestId("preflight-error")).toHaveProperty("textContent", "preflight boom");
    expect(screen.getByTestId("plan-list")).toBeTruthy();
  });
});

describe("MacroRunConfirm collision confirm-and-retry (CPE-1891)", () => {
  it("lists a confirmable collision, disables Run until the overwrite box is checked, warns it's not undoable, then runs with that destination named", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [CONFIRMABLE_COLLISION];
      if (cmd === "macro_run") return RESOLVED_RUN;
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    const list = await screen.findByTestId("confirmable-collisions");
    expect(list.textContent).toContain("/work/a_v2.txt");
    expect(screen.queryByTestId("blocked-collisions")).toBeNull();
    // Blocker 3 (PR #1044 review round 2): the confirm panel must warn this isn't reversible.
    expect(screen.getByTestId("irreversible-note").textContent).toMatch(/can.t be undone/i);

    const runBtn = screen.getByTestId("run-btn") as HTMLButtonElement;
    expect(runBtn.disabled).toBe(true);
    expect(runBtn.textContent).toContain("Run");

    await fireEvent.click(screen.getByTestId("confirm-overwrite"));
    expect(runBtn.disabled).toBe(false);
    expect(runBtn.textContent).toContain("Overwrite 1 and Run");

    await fireEvent.click(runBtn);
    // Blocker 2: the CONFIRMED DESTINATION, not a bare `true` -- the backend only bypasses the
    // occupancy guard at a `to` actually named here.
    expect(invokeMock).toHaveBeenCalledWith("macro_run", {
      macro: MACRO,
      inputs: ["/work/a.txt"],
      root: "/work",
      confirmedOverwrite: ["/work/a_v2.txt"],
    });
  });

  it("lists a blocked (link) collision with no checkbox, and Run stays disabled regardless", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [BLOCKED_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    const list = await screen.findByTestId("blocked-collisions");
    expect(list.textContent).toContain("/work/a_v2.txt");
    expect(screen.queryByTestId("confirmable-collisions")).toBeNull();
    expect(screen.queryByTestId("confirm-overwrite")).toBeNull();
    expect((screen.getByTestId("run-btn") as HTMLButtonElement).disabled).toBe(true);
  });

  it("renders ONE reason sentence per distinct hazard kind, hoisted above the path list -- not one per row", async () => {
    // CPE-1891 Visual Critic pass: the earlier per-row placement repeated the same paragraph once per
    // path (the wording is per-KIND, not per-path) and clipped mid-sentence past a handful of rows.
    // Three blocked items: rename + move (share the "destroys it" wording) + convert ("writes THROUGH
    // it") -- must collapse to exactly TWO reason sentences, not three, and both must actually reach the
    // DOM.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [BLOCKED_COLLISION, CONVERT_BLOCKED_COLLISION, MOVE_BLOCKED_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt", "/work/b.png", "/work/c.txt"], root: "/work" });
    await screen.findByTestId("blocked-collisions");
    const reasons = screen.getAllByTestId("blocked-reason").map((el) => el.textContent);

    expect(reasons).toHaveLength(2);
    // CPE-1928: pinned as whole strings, in first-appearance order. Each is the OTHER's negative case
    // too -- the pair are asserted distinct below, so neither assertion can be satisfied by the wrong
    // hazard's sentence.
    expect(reasons[0]).toEqual(RENAME_HAZARD_SENTENCE);
    expect(reasons[1]).toEqual(CONVERT_HAZARD_SENTENCE);
    expect(RENAME_HAZARD_SENTENCE).not.toEqual(CONVERT_HAZARD_SENTENCE);
    // ...and distinct where it matters: at the START, which is the whole point of CPE-1928. A reader
    // who takes in only the first few words must already know which hazard this is, so the sentences
    // are required to diverge inside their opening run of words, not merely somewhere.
    const opening = (s: string) => s.split(" ").slice(0, 4).join(" ");
    expect(opening(RENAME_HAZARD_SENTENCE)).not.toEqual(opening(CONVERT_HAZARD_SENTENCE));
    // The shared remedy is stated ONCE, below both, and is absent from each hazard sentence -- the
    // duplicated closing clause was half of what made the two read as one paragraph twice.
    expect(screen.getAllByTestId("blocked-remedy")).toHaveLength(1);
    expect(screen.getByTestId("blocked-remedy").textContent).toEqual(SHARED_REMEDY);
    for (const reason of reasons) expect(reason).not.toContain("remove the link first");
    // MOVE_BLOCKED_COLLISION shares BLOCKED_COLLISION's bucket and therefore its one rendered sentence
    // (same "destroys it" wording, only their raw paths differed) -- so its own reason text has nothing
    // further to assert once the path is stripped; that it is folded in is what `toHaveLength(2)` says.
    expect(MOVE_BLOCKED_COLLISION.reason).toContain("renaming onto a link destroys it");
    // The two variants must actually read differently -- a shared generic sentence would defeat the
    // point (rename/move "destroys" the link; convert "writes THROUGH" it).
    expect(screen.getByTestId("blocked-collisions").textContent).toContain("destroys it");
    expect(screen.getByTestId("blocked-collisions").textContent).toContain("writes THROUGH it");
    // Visual Critic round 3: the hoisted sentence must not name any ONE collision's own path -- with
    // several blocked items that made the sentence look tied to just the first one.
    for (const reason of reasons) {
      expect(reason).not.toContain(BLOCKED_COLLISION.to);
      expect(reason).not.toContain(CONVERT_BLOCKED_COLLISION.to);
      expect(reason).not.toContain(MOVE_BLOCKED_COLLISION.to);
    }
    // All three PATHS still appear in the plain path list below the reason(s), including the
    // move-kind item whose own reason text was folded into the rename/move sentence above.
    expect(screen.getByTestId("blocked-collisions").textContent).toContain("/work/Archive/c.txt");
  });

  it("with only ONE hazard kind, folds the shared remedy back into that sentence instead of leaving a lone line", async () => {
    // CPE-1928's other half: the remedy is hoisted out because TWO sentences repeating it read as one
    // paragraph twice. With a single sentence there is nothing to de-duplicate, and a remedy stranded on
    // its own line below would read as a second, contentless hazard -- so it closes its own sentence.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [CONVERT_BLOCKED_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/b.png"], root: "/work" });
    await screen.findByTestId("blocked-collisions");
    const reasons = screen.getAllByTestId("blocked-reason").map((el) => el.textContent);

    expect(reasons).toHaveLength(1);
    expect(reasons[0]).toEqual(`${CONVERT_HAZARD_SENTENCE} ${SHARED_REMEDY}`);
    // Specifically the CONVERT hazard, not the rename one -- the fold must not launder the two into a
    // single generic sentence, which is the failure mode this whole area keeps circling.
    expect(reasons[0]).toContain("writes THROUGH it");
    expect(reasons[0]).not.toContain("destroys it");
    expect(reasons[0]).not.toEqual(`${RENAME_HAZARD_SENTENCE} ${SHARED_REMEDY}`);
    expect(screen.queryByTestId("blocked-remedy")).toBeNull();
    // ...and the remedy is still SAID -- folding it in must not drop it.
    expect(screen.getByTestId("blocked-collisions").textContent).toContain("remove the link first");
    // The path stays the list's job, not the sentence's (CPE-1891).
    expect(reasons[0]).not.toContain(CONVERT_BLOCKED_COLLISION.to);
  });

  it("keeps the remedy on screen when the reason's LEAD drifts but its tail still matches", async () => {
    // PR #1056 review, Finding 1. The two halves of the split drift INDEPENDENTLY, and one of the two
    // drift directions used to lose the remedy outright: strip-the-tail-then-test-the-lead deleted
    // "remove the link first ..." from the sentence and THEN fell through to `genericizeReason`, while
    // `representativeReasons` -- which tests the lead against the RAW reason -- reported no link hazard
    // and so rendered no separate remedy line either. Net effect: the one thing the user has to DO
    // disappeared from the dialog. Not hypothetical: `batch_media.rs` already prefixes this same
    // refusal with "refusing at write time: ", so a lead-drifted reason is a string the backend can
    // produce. The fix is to recognise the lead on the raw reason and strip the tail only after.
    const LEAD_DRIFTED = {
      ...BLOCKED_COLLISION,
      reason: `refusing at write time: ${BLOCKED_COLLISION.reason}`,
    };
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [LEAD_DRIFTED];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("blocked-collisions");

    // THE assertion this test exists for: the remedy is still somewhere on screen.
    expect(screen.getByTestId("blocked-collisions").textContent).toContain("remove the link first");
    // ...and specifically because the sentence came through UNTOUCHED -- an unrecognised reason is
    // passed on verbatim, remedy included, rather than half-processed.
    expect(screen.getByTestId("blocked-reason").textContent).toEqual(LEAD_DRIFTED.reason);
    // No separate remedy line here (the shape was not recognised, so nothing was hoisted) -- which is
    // exactly why the sentence must keep its own tail. Asserting BOTH halves pins the invariant that
    // matters: the remedy appears exactly once, wherever it appears.
    expect(screen.queryByTestId("blocked-remedy")).toBeNull();
    const remedyCount = (screen.getByTestId("blocked-collisions").textContent ?? "").split(
      "remove the link first",
    ).length - 1;
    expect(remedyCount).toBe(1);
  });

  it("shows a dim note stating why Run is blocked, next to the button, when a link collision is present", async () => {
    // CPE-1891, Visual Critic round 3: the red box's explanation lives two panels above the button --
    // this states the SAME fact right where the user is looking when they wonder why Run won't light.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [BLOCKED_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("blocked-collisions");
    expect(screen.getByTestId("run-blocked-note").textContent).toContain("Run is blocked by 1 link above");
  });

  it("does NOT show the run-blocked note when there is nothing blocking Run", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [CONFIRMABLE_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("confirmable-collisions");
    expect(screen.queryByTestId("run-blocked-note")).toBeNull();
  });

  it("copies every blocked destination name to the clipboard from its own copy button", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [BLOCKED_COLLISION, CONVERT_BLOCKED_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt", "/work/b.png"], root: "/work" });
    const btn = await screen.findByTestId("copy-blocked-collisions");
    expect(btn.textContent).toContain("Copy all 2 names");

    await fireEvent.click(btn);
    expect(writeText).toHaveBeenCalledWith('"/work/a_v2.txt"\n"/work/b.jpg"');
    expect(await screen.findByText("Copied")).toBeTruthy();
  });

  it("a mix of confirmable and blocked collisions still refuses Run, and keeps the plain 'Run' label, even once the confirmable one is checked", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [CONFIRMABLE_COLLISION, { ...BLOCKED_COLLISION, op_index: 1 }];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("confirmable-collisions");
    await screen.findByTestId("blocked-collisions");

    await fireEvent.click(screen.getByTestId("confirm-overwrite"));
    const runBtn = screen.getByTestId("run-btn") as HTMLButtonElement;
    expect(runBtn.disabled).toBe(true);
    // Should-fix (PR #1044 review round 2): a still-blocked run must not read as armed-and-ready.
    expect(runBtn.textContent?.trim()).toBe("Run");
    expect(runBtn.textContent).not.toContain("Overwrite");
  });

  it("copies every confirmable collision's destination name to the clipboard (CPE-1869's reused affordance)", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [CONFIRMABLE_COLLISION];
      return null;
    });

    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    const btn = await screen.findByTestId("copy-collisions");
    expect(btn.textContent).toContain("Copy all 1 name");

    await fireEvent.click(btn);
    expect(writeText).toHaveBeenCalledWith('"/work/a_v2.txt"');
    expect(await screen.findByText("Copied")).toBeTruthy();
  });
});

describe("MacroRunConfirm run results + undo (CPE-1191)", () => {
  it("after a successful run, shows the applied step count and offers Undo -> macro_undo", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "macro_plan") return PLAN;
      if (cmd === "macro_preflight") return [];
      if (cmd === "macro_run") return RESOLVED_RUN;
      if (cmd === "macro_undo") return null;
      return null;
    });
    render(MacroRunConfirm, { macro: MACRO, inputs: ["/work/a.txt"], root: "/work" });
    await screen.findByTestId("plan-list");
    await fireEvent.click(screen.getByTestId("run-btn"));

    const results = await screen.findByTestId("run-results");
    expect(results.textContent).toContain("Applied 1 step");
    expect(screen.getByTestId("undo-btn")).toBeTruthy();

    await fireEvent.click(screen.getByTestId("undo-btn"));
    expect(invokeMock).toHaveBeenCalledWith("macro_undo", { run: RESOLVED_RUN });
    expect(await screen.findByTestId("undone-note")).toBeTruthy();
    expect(screen.queryByTestId("undo-btn")).toBeNull();
  });
});

/**
 * PR #1056 review, Finding 2 — the derivation guard.
 *
 * Everything above compares frontend constants to frontend fixtures, both hand-copied from Rust. The
 * Rust side pins nothing either: every backend assertion on these two messages is a SUBSTRING check
 * (`fsutil.rs`, `src-tauri/src/lib.rs`), none of which reds on a lead reword. So a backend copy edit
 * could pass `cargo test`, pass `npm test`, and silently change what this dialog renders — the exact
 * class of drift Finding 1 showed can DELETE the remedy rather than merely restyle it.
 *
 * This reads `crates/server/src/fsutil.rs` and re-derives both messages from the `format!` literals
 * themselves, so the drift reds in CI on the side that caused it. Same shape as this repo's other
 * source-reading guards (`channelPurityCoverage`, `catalogPublishFreshnessGuard`, `lockfileLockedGuard`).
 */
describe("blocked-reason fixtures are DERIVED from the Rust guards, not hand-copied (PR #1056, Finding 2)", () => {
  // CPE-1950: `stripRustComments` and `rustStringLiteralAfter` were written here and now live in
  // `src/lib/rustSource.ts`, imported at the top of this file, so the other Rust-source scanners
  // (`sidecarBundleResources.test.ts`, `RepoBrowser.test.ts`) reuse this exact escape/comment handling
  // instead of becoming a fifth hand-rolled stripper. Their doc comments moved with them; the two
  // adversarial cases below still run here, against the shared implementation.

  /** The `Ok(true)` (it IS a link) arm's `format!` template out of the named function. */
  function linkRefusalTemplate(src: string, fnName: string): string {
    const fnStart = src.indexOf(`pub fn ${fnName}`);
    expect(fnStart, `${fnName} not found in fsutil.rs`).toBeGreaterThan(-1);
    const fmt = src.indexOf("format!(", fnStart);
    expect(fmt, `${fnName} has no format! call`).toBeGreaterThan(-1);
    return rustStringLiteralAfter(src, fmt);
  }

  const FSUTIL = stripRustComments(
    readFileSync(join(process.cwd(), "crates", "server", "src", "fsutil.rs"), "utf8"),
  );


  // CPE-1933: the adversarial source PR #1056's Reviewer found that beat the anchor SILENTLY. A
  // comment between the signature and the real `format!(`, quoting the OLD message. Without
  // `stripRustComments` the extractor returns the stale text and the fixture "derives" clean.
  it("a comment quoting the OLD message cannot be mistaken for the real format! call", () => {
    const hostile = [
      "pub fn classify_symlink_slot(x: u8) -> Result<bool> {",
      '    // Historical note: this used to be format!("the OLD wording {}", dst).',
      '    Ok(format!("the CURRENT wording {}", dst))',
      "}",
    ].join("\n");
    const derived = linkRefusalTemplate(stripRustComments(hostile), "classify_symlink_slot");
    expect(
      derived,
      "the extractor read the message out of a COMMENT -- exactly the hole this stripping closes",
    ).toEqual("the CURRENT wording {}");
  });

  it("a // inside a string literal is not mistaken for a comment", () => {
    const withUrl = [
      "pub fn classify_create_slot(x: u8) -> Result<bool> {",
      '    Ok(format!("see https://example.com/docs {}", dst))',
      "}",
    ].join("\n");
    expect(linkRefusalTemplate(stripRustComments(withUrl), "classify_create_slot")).toEqual(
      "see https://example.com/docs {}",
    );
  });

  it("the rename/move fixture is byte-identical to `classify_symlink_slot`'s own message", () => {
    const derived = linkRefusalTemplate(FSUTIL, "classify_symlink_slot").replace("{}", BLOCKED_COLLISION.to);
    expect(derived).toEqual(BLOCKED_COLLISION.reason);
    // The move-kind fixture is the same message at a different path -- derive it too rather than
    // assuming, since it is what proves the dedup buckets on wording and not on luck.
    const derivedMove = linkRefusalTemplate(FSUTIL, "classify_symlink_slot").replace("{}", MOVE_BLOCKED_COLLISION.to);
    expect(derivedMove).toEqual(MOVE_BLOCKED_COLLISION.reason);
  });

  it("the convert fixture is byte-identical to `classify_create_slot`'s own message", () => {
    const derived = linkRefusalTemplate(FSUTIL, "classify_create_slot").replace("{}", CONVERT_BLOCKED_COLLISION.to);
    expect(derived).toEqual(CONVERT_BLOCKED_COLLISION.reason);
  });

  it("both Rust messages still END with the clause SHARED_REMEDY claims to replace", () => {
    // SHARED_REMEDY is the component's own wording, not a copy of either Rust tail -- it deliberately
    // says "changed" for both. What has to hold is that each Rust message really does close on this
    // clause (so there IS one shared remedy to hoist), and that the two differ only in that one word.
    const rename = linkRefusalTemplate(FSUTIL, "classify_symlink_slot");
    const convert = linkRefusalTemplate(FSUTIL, "classify_create_slot");
    expect(rename).toContain("Nothing was changed; remove the link first if that is what you meant");
    expect(convert).toContain("Nothing was written; remove the link first if that is what you meant");
    expect(rename.endsWith("remove the link first if that is what you meant")).toBe(true);
    expect(convert.endsWith("remove the link first if that is what you meant")).toBe(true);
    // ...and the wording the UI prints once is one of those two, modulo the changed/written choice.
    expect(`${rename.slice(rename.indexOf("Nothing was changed"))}.`).toEqual(SHARED_REMEDY);
    // The clause appears exactly ONCE per message -- if it ever appeared twice, hoisting one copy out
    // would leave the other stranded in the sentence and the remedy would render twice.
    expect(rename.split("remove the link first").length - 1).toBe(1);
    expect(convert.split("remove the link first").length - 1).toBe(1);
  });

  it("both Rust messages still OPEN with the lead the splitter strips", () => {
    // The half Finding 1 turned on. If the Rust lead is reworded, this reds HERE -- on the Rust
    // change -- instead of the dialog quietly rendering a differently-shaped sentence.
    for (const fnName of ["classify_symlink_slot", "classify_create_slot"]) {
      expect(linkRefusalTemplate(FSUTIL, fnName), fnName).toMatch(/^"\{\}" is a link, and /);
    }
  });
});

/**
 * CPE-1983 — the operations list's height does not depend on the resolved plan.
 *
 * `onMount` resolves the macro's plan (`macro_plan` + `macro_preflight`), so `.ops` is empty when the
 * confirm appears and full a moment later. `.backdrop` centres the dialog, so that growth slid the
 * Run/Cancel row and the warning text apart under the pointer — CPE-1968's shape, in a dialog whose
 * whole purpose is a deliberate confirmation.
 *
 * TWO PROPERTIES HOLD THE HEIGHT AND THEY ARE ASSERTED SEPARATELY, because removing both at once only
 * proves the pair (CPE-1968 measured exactly this on `MacrosDialog`). `.dialog` is a flex column with
 * its own `max-height`, so a flex item's default `flex-shrink: 1` lets the free-space algorithm
 * override a declared `height` the moment the dialog reaches that cap. `flex: 0 0 auto` is what makes
 * the height actually hold.
 *
 * RED-PROOF, run and recorded here rather than only in the PR body (CPE-1933 rule 3):
 *   - reverting `.ops` to `max-height: 40vh` with no `height` reds **1 of 2** in this block (the
 *     height leg) **plus** the repo-wide `src/lib/dialogBodyReflow.test.ts` leg naming
 *     `MacroRunConfirm.svelte#ops` — 2 of 40 across the two files;
 *   - with the `height` in place and `flex: 0 0 auto` alone deleted, **1 of 2** reds — the flex leg
 *     only, and the repo-wide guard stays GREEN. So the flex assertion is not decorative, and it is
 *     also the half no enumerating guard covers: without it the flex column shrinks the fixed height
 *     back toward content and undoes the fix while the `height` declaration still reads correct.
 * Restored; both green.
 */
describe("CPE-1983 — the plan list's height does not depend on the resolved plan", () => {
  const SRC = readFileSync(join(process.cwd(), "src", "lib", "components", "MacroRunConfirm.svelte"), "utf8");
  /** `src-tauri/src/lib.rs`'s `.inner_size(1000.0, 700.0)`; only the vh terms below read it. */
  const VIEWPORT_H = 700;

  it("gives .ops a content-independent height, so the plan landing cannot move Run/Cancel", () => {
    const ops = styleBlock(SRC, "ops");
    expect(
      declaration(ops, "max-height"),
      "`.ops` declares a max-height again with no matching height — the CPE-1983 shape: the box is " +
        "empty while `macro_plan` is in flight and up to the cap once it resolves, so the centred " +
        "dialog moves the confirm buttons under the pointer.",
    ).toBeUndefined();

    const reason = contentIndependentHeightReason(ops, VIEWPORT_H);
    expect(reason, `\`.ops\` ${reason}. See CPE-1983 and src/lib/dialogBodyReflow.test.ts.`).toBeNull();
  });

  it("keeps that height fixed under flex, which would otherwise shrink it back to its content", () => {
    expect(declaration(styleBlock(SRC, "dialog"), "display")).toMatch(/flex/);
    expect(
      declaration(styleBlock(SRC, "ops"), "flex"),
      "`.ops` needs `flex: 0 0 auto` — inside `.dialog`'s flex column a shrinkable item falls back " +
        "toward its content height, which reintroduces the growth the fixed height removes",
    ).toMatch(/^0\s+0\b/);
  });
});
