// CPE-1869 verification harness script. Mounts the REAL `RevertOutcomePanel.svelte` three times with
// canned `RevertOutcome` fixtures (see index.html's header for what each case proves), sets the theme
// from `?theme=`, and — when `?autoclick=1` is present — dispatches a real click on case 1's copy
// button shortly after mount so a screenshot can capture the post-click "Copied" confirmation state
// (rather than only the pristine "Copy all N held-back paths" state).
//
// `navigator.clipboard` is stubbed with an in-memory fake BEFORE mount, the same way the vitest suite
// (CheckpointDialog.test.ts) mocks it — real Clipboard-API writes need a permission grant this bare
// `chrome --headless=new --screenshot` invocation has no way to hand out, and the component's own
// production code already treats a rejected write as "clipboard unavailable" and silently leaves the
// panel showing the pristine label (see RevertOutcomePanel.svelte's `copyHeldBackPaths` catch clause) —
// so an unstubbed clipboard would prove nothing about the UI's *own* correctness, only about headless
// Chrome's permission model.
import RevertOutcomePanel from "../../../src/lib/components/RevertOutcomePanel.svelte";
import type { RevertOutcome } from "../../../src/lib/bindings.gen";

const params = new URLSearchParams(location.search);
const theme = params.get("theme") === "dark" ? "dark" : "light";
document.documentElement.dataset.theme = theme;
const autoclick = params.get("autoclick") === "1";

let lastClipboardWrite = "";
Object.defineProperty(navigator, "clipboard", {
  value: {
    writeText: async (text: string) => {
      lastClipboardWrite = text;
    },
  },
  configurable: true,
});

// 1. Unrestorable-key: a non-empty checkpoint holds SOME entries this platform cannot restore. Real
// prose lifted from `revert_engine.rs`'s own branch (not reworded here — this is the actual wire text).
const unrestorablePaths = Array.from({ length: 23 }, (_, i) => `assets/added-${String(i).padStart(2, "0")}.png`);
const unrestorableOutcome: RevertOutcome = {
  applied: 4,
  skipped: unrestorablePaths.map((path) => ({
    path,
    ok: false,
    error: "",
    outcome: "held_back_by_checkpoint" as const,
  })),
  held_back: {
    outcome: "held_back_by_checkpoint",
    count: unrestorablePaths.length,
    reason:
      '1 of this checkpoint’s entries cannot be restored on this computer ("notes: draft.txt"), ' +
      'so "this file is not in the checkpoint" cannot be trusted — deleting it may destroy a file ' +
      "the checkpoint does hold, under a name spelled differently here.",
    next_step:
      "There is no fix for this on this computer: those names are stored in the checkpoint and this " +
      "filesystem cannot write them, so re-running the revert will hold the same files back again. " +
      "Everything restorable has already been restored. Delete these files yourself if you want them " +
      "gone, or finish the revert on the system the checkpoint was captured on. The full list is below " +
      "— copy it to work through the files.",
    retryable: false,
    advises_manual_delete: true,
  },
};

// 2. Alias/collision: the held-back paths ARE the checkpoint's own content, reached under another
// spelling on this volume. Must NOT get the copy affordance.
const aliasOutcome: RevertOutcome = {
  applied: 0,
  skipped: [
    {
      path: "Reports/Q1.txt",
      ok: false,
      error: 'same file as checkpoint entry "reports/q1.txt"',
      outcome: "held_back_by_checkpoint",
    },
  ],
  held_back: {
    outcome: "held_back_by_checkpoint",
    count: 1,
    reason:
      "These paths resolve to the same files as entries the checkpoint already holds, spelled " +
      'differently. "This file is not in the checkpoint" is true of the spelling and false of the ' +
      "file, so deleting them would destroy content the checkpoint is there to protect.",
    next_step:
      "Nothing needs doing and re-running will not change it: these files ARE the checkpoint's own " +
      "content, reached under another spelling on this volume, so they are already in the state the " +
      "revert was asking for.",
    retryable: false,
    advises_manual_delete: false,
  },
};

// 3. Retryable: a locked file / missing blob this run. Must NOT get the copy affordance either —
// nothing needs deleting YET.
const retryableOutcome: RevertOutcome = {
  applied: 3,
  skipped: [{ path: "cache/thumb.db", ok: false, error: "", outcome: "skipped_by_plan" }],
  held_back: {
    outcome: "skipped_by_plan",
    count: 1,
    reason: '1 checkpoint entry could not be restored this time ("cache/thumb.db"), so "this file is not in the checkpoint" cannot be trusted yet.',
    next_step:
      "This one is temporary: close whatever is holding those files (or restore the missing stored " +
      "content) and run the revert again — the held-back cleanups will then apply.",
    retryable: true,
    advises_manual_delete: false,
  },
};

new RevertOutcomePanel({
  target: document.getElementById("mount-unrestorable")!,
  props: { outcome: unrestorableOutcome, testid: "case-unrestorable", verb: "Reverted", root: "/work/proj" },
});
new RevertOutcomePanel({
  target: document.getElementById("mount-alias")!,
  props: { outcome: aliasOutcome, testid: "case-alias", verb: "Reverted", root: "/work/proj" },
});
new RevertOutcomePanel({
  target: document.getElementById("mount-retryable")!,
  props: { outcome: retryableOutcome, testid: "case-retryable", verb: "Reverted", root: "/work/proj" },
});

function computeDiag() {
  const copyBtn = (id: string) => document.querySelector(`[data-testid="${id}-copy-held-paths"]`);
  const diag = {
    theme,
    autoclick,
    case1_copyButtonPresent: !!copyBtn("case-unrestorable"),
    case1_copyButtonText: copyBtn("case-unrestorable")?.textContent?.trim() ?? null,
    case2_copyButtonPresent: !!copyBtn("case-alias"),
    case3_copyButtonPresent: !!copyBtn("case-retryable"),
    lastClipboardWrite,
  };
  (window as unknown as { __revertHarnessDiag?: unknown }).__revertHarnessDiag = diag;
  const el = document.getElementById("readout");
  if (el) el.textContent = JSON.stringify(diag, null, 2);
}

requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    if (autoclick) {
      const btn = document.querySelector(
        '[data-testid="case-unrestorable-copy-held-paths"]',
      ) as HTMLButtonElement | null;
      btn?.click();
      // Let the click's async writeText + Svelte's reactive re-render settle before the screenshot.
      setTimeout(computeDiag, 100);
    } else {
      computeDiag();
    }
  });
});
