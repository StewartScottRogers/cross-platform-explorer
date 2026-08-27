// CPE-1891 verification harness script. See index.html's header for what `?case=`/`?theme=` pick.
//
// The shim: `@tauri-apps/api/core`'s real `invoke(cmd, args, options)` body is exactly
// `return window.__TAURI_INTERNALS__.invoke(cmd, args, options);` (checked against the installed
// package) — so defining that ONE global before `MacroRunConfirm` (and therefore `invoke.ts` and
// `bindings.gen.ts`) ever import `@tauri-apps/api/core` makes every `commands.*` call in this harness
// run the REAL generated client and the REAL busy-cursor wrapper, with only the OS IPC boundary
// replaced — not a mocked module, not an aliased import.
import type { ActionMacro, MacroCollision, PlannedOp } from "../../../src/lib/bindings.gen";

const params = new URLSearchParams(location.search);
const theme = params.get("theme") === "dark" ? "dark" : "light";
document.documentElement.dataset.theme = theme;
const kaseParam = params.get("case");
const kase = kaseParam === "mixed" ? "mixed" : kaseParam === "many" ? "many" : "blocked";

const MACRO: ActionMacro = { name: "Tidy up", steps: [{ rename: { template: "{stem}_v2.{ext}" } }] };

const BLOCKED_ONLY_PLAN: PlannedOp[] = [
  { input: "/Users/proj/photos/vacation.jpg", kind: "rename", detail: "/Users/proj/photos/vacation_v2.jpg" },
];
const BLOCKED_ONLY: MacroCollision[] = [
  {
    op_index: 0,
    from: "/Users/proj/photos/vacation.jpg",
    to: "/Users/proj/photos/vacation_v2.jpg",
    kind: "rename",
    confirmable: false,
    reason:
      '"/Users/proj/photos/vacation_v2.jpg" is a link, and renaming onto a link destroys it — the link ' +
      "is removed and its target is left orphaned. Nothing was changed; remove the link first if that " +
      "is what you meant",
  },
];

const MIXED_PLAN: PlannedOp[] = [
  { input: "/Users/proj/photos/vacation.jpg", kind: "rename", detail: "/Users/proj/photos/vacation_v2.jpg" },
  { input: "/Users/proj/photos/sunset.png", kind: "convert", detail: "jpg" },
];
const MIXED: MacroCollision[] = [
  {
    op_index: 0,
    from: "/Users/proj/photos/vacation.jpg",
    to: "/Users/proj/photos/vacation_v2.jpg",
    kind: "rename",
    confirmable: true,
    reason: '"vacation_v2.jpg" already exists',
  },
  {
    op_index: 1,
    from: "/Users/proj/photos/sunset.png",
    to: "/Users/proj/photos/sunset.jpg",
    kind: "convert",
    confirmable: false,
    reason:
      '"/Users/proj/photos/sunset.jpg" is a link, and creating a file at a link\'s name writes THROUGH ' +
      "it — the bytes would land at the link's target, a path you did not name, and a failure part-way " +
      "would then delete the link itself. Nothing was written; remove the link first if that is what " +
      "you meant",
  },
];

// `?case=many` — Visual Critic round 3, item 1: THREE blocked collisions (rename + move, sharing the
// "destroys it" bucket, plus a convert with its own "writes THROUGH it" bucket) so the hoisted
// per-kind reason sentence's genericisation is actually evidenced: the sentence must read as a general
// statement about the KIND, not name only the first item's path while the list below shows three
// different ones.
const MANY_PLAN: PlannedOp[] = [
  { input: "/Users/proj/photos/vacation.jpg", kind: "rename", detail: "/Users/proj/photos/vacation_v2.jpg" },
  { input: "/Users/proj/photos/beach.jpg", kind: "move", detail: "/Users/proj/photos/Archive" },
  { input: "/Users/proj/photos/sunset.png", kind: "convert", detail: "jpg" },
];
const MANY: MacroCollision[] = [
  {
    op_index: 0,
    from: "/Users/proj/photos/vacation.jpg",
    to: "/Users/proj/photos/vacation_v2.jpg",
    kind: "rename",
    confirmable: false,
    reason:
      '"/Users/proj/photos/vacation_v2.jpg" is a link, and renaming onto a link destroys it — the link ' +
      "is removed and its target is left orphaned. Nothing was changed; remove the link first if that " +
      "is what you meant",
  },
  {
    op_index: 1,
    from: "/Users/proj/photos/beach.jpg",
    to: "/Users/proj/photos/Archive/beach.jpg",
    kind: "move",
    confirmable: false,
    reason:
      '"/Users/proj/photos/Archive/beach.jpg" is a link, and renaming onto a link destroys it — the ' +
      "link is removed and its target is left orphaned. Nothing was changed; remove the link first if " +
      "that is what you meant",
  },
  {
    op_index: 2,
    from: "/Users/proj/photos/sunset.png",
    to: "/Users/proj/photos/sunset.jpg",
    kind: "convert",
    confirmable: false,
    reason:
      '"/Users/proj/photos/sunset.jpg" is a link, and creating a file at a link\'s name writes THROUGH ' +
      "it — the bytes would land at the link's target, a path you did not name, and a failure part-way " +
      "would then delete the link itself. Nothing was written; remove the link first if that is what " +
      "you meant",
  },
];

const plan = kase === "mixed" ? MIXED_PLAN : kase === "many" ? MANY_PLAN : BLOCKED_ONLY_PLAN;
const collisions = kase === "mixed" ? MIXED : kase === "many" ? MANY : BLOCKED_ONLY;

(window as unknown as { __TAURI_INTERNALS__: { invoke: (cmd: string, args?: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__ = {
  invoke: async (cmd: string) => {
    if (cmd === "macro_plan") return plan;
    if (cmd === "macro_preflight") return collisions;
    throw new Error(`[CPE-1891 harness] unmocked command: ${cmd}`);
  },
};

Object.defineProperty(navigator, "clipboard", {
  value: { writeText: async () => {} },
  configurable: true,
});

const { default: MacroRunConfirm } = await import("../../../src/lib/components/MacroRunConfirm.svelte");

new MacroRunConfirm({
  target: document.body,
  props: { macro: MACRO, inputs: plan.map((p) => p.input), root: "/Users/proj" },
});

function computeDiag() {
  const diag = {
    theme,
    case: kase,
    blockedPresent: !!document.querySelector('[data-testid="blocked-collisions"]'),
    confirmablePresent: !!document.querySelector('[data-testid="confirmable-collisions"]'),
    runBtnDisabled: (document.querySelector('[data-testid="run-btn"]') as HTMLButtonElement | null)?.disabled,
    runBtnText: document.querySelector('[data-testid="run-btn"]')?.textContent?.trim(),
  };
  (window as unknown as { __macroHarnessDiag?: unknown }).__macroHarnessDiag = diag;
  const el = document.getElementById("readout");
  if (el) el.textContent = JSON.stringify(diag, null, 2);
  document.body.dataset.harnessReady = "true";
}

// Visual Critic round 3, item 3: the dark captures showed the readout stuck on "booting…" — the pixels
// were still correct (theme/layout render synchronously with mount), but this diagnostic overlay update
// is chained behind two rAFs + a timeout and evidently did not always settle inside the screenshot
// tool's window. Widened from one 100ms timeout to a short RETRYING poll (checked every 50ms, up to
// 2s) rather than a single longer guess, so it settles as soon as the DOM is actually ready instead of
// padding every capture with dead time.
function settleThenDiag(attempt = 0) {
  const ready =
    document.querySelector('[data-testid="run-btn"]') !== null &&
    (kase !== "mixed" || (document.querySelector('[data-testid="confirm-overwrite"]') as HTMLInputElement | null)?.checked);
  if (ready || attempt > 40) {
    computeDiag();
    return;
  }
  setTimeout(() => settleThenDiag(attempt + 1), 50);
}

requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    if (kase === "mixed") {
      // Tick the confirm checkbox so the screenshot shows the should-fix in its actually-interesting
      // state: checked, but Run must stay disabled and labelled plain "Run" (never "Overwrite N and
      // Run") while a blocked collision is still present.
      const box = document.querySelector('[data-testid="confirm-overwrite"]') as HTMLInputElement | null;
      if (box) {
        box.checked = true;
        box.dispatchEvent(new Event("change", { bubbles: true }));
      }
    }
    settleThenDiag();
  });
});
