// CPE-1983 layout harness — mounts the REAL CheckpointDialog.svelte against a canned checkpoint list,
// so the dialog's reflow (or, after the fix, its absence) can be measured and photographed by a real
// layout engine. See index.html's header for the query parameters and for why this page exists at
// all; see src/lib/components/CheckpointDialog.svelte's `.list` comment for the defect itself.
import { registerCommand } from "../layout-guard/shared-mocks/bindings.gen";
import CheckpointDialog from "../../../src/lib/components/CheckpointDialog.svelte";

const params = new URLSearchParams(location.search);
document.documentElement.dataset.theme = params.get("theme") === "dark" ? "dark" : "light";

const listSize = params.get("list") ?? "few";
const delayMs = Number(params.get("delay") ?? "0");
const legacy = params.get("legacy") === "1";

/**
 * The PRE-CPE-1983 `.list` height, re-applied over the shipped one so the "before" screenshots come
 * out of the same working tree as the "after" ones. `!important` because Svelte's scoped class
 * (`.list.svelte-xxxx`) outranks a bare attribute selector from outside the component — this is a
 * deliberate override, not a style the app has.
 */
if (legacy) {
  const style = document.createElement("style");
  style.textContent = `[data-testid="checkpoint-list"] {
    height: auto !important;
    max-height: 30vh !important;
  }`;
  document.head.append(style);
}

const LABELS = [
  "before refactor",
  "pre-upgrade",
  "clean tree",
  "before batch rename",
  "nightly",
  "before auto-organize",
];

/** Fixtures. `few` is the "is this box absurdly empty?" case a Visual Critic is asked about; `many`
 *  overflows any box this dialog could reasonably have and is what the scroll viewport is really for. */
const LISTS: Record<string, { manifest_id: string; label: string; ts: number }[]> = {
  none: [],
  few: [
    { manifest_id: "c1f2a3b4c5d6e7f8", label: "before refactor", ts: Date.UTC(2026, 7, 27, 14, 30) },
    { manifest_id: "a9b8c7d6e5f40312", label: "pre-upgrade", ts: Date.UTC(2026, 7, 26, 9, 5) },
  ],
  many: Array.from({ length: 12 }, (_, i) => ({
    manifest_id: `${(i + 1).toString(16).padStart(2, "0")}f0e1d2c3b4a596${i}`,
    label: LABELS[i % LABELS.length],
    ts: Date.UTC(2026, 7, 27 - i, 8 + (i % 12), (i * 7) % 60),
  })),
};

const checkpoints = LISTS[listSize] ?? LISTS.few;

// `listResolved` is recorded HERE, at the mock, rather than inferred from the DOM — the same reason
// organize-dialog's probe does: no rendered testid distinguishes "still loading" from "landed empty",
// because `.list` renders one of three `.empty` placeholders and the mock is the only place that
// knows for certain which state produced it.
let listResolved = false;
registerCommand("checkpointList", async () => {
  await new Promise((r) => setTimeout(r, delayMs));
  listResolved = true;
  return checkpoints;
});
registerCommand("checkpointFailuresList", async () => {
  await new Promise((r) => setTimeout(r, delayMs));
  return [];
});

new CheckpointDialog({
  target: document.getElementById("mount")!,
  props: { initialPath: "/home/user/projects/cross-platform-explorer" },
});

// The probe. Each screenshot carries its own measurement of where Refresh actually is, so a
// before/after pair is a pair of NUMBERS rather than two pictures to eyeball. `firstTop` is sampled at
// t=100ms — the window in which a user's pointer is already on the button and the list has not landed.
//
// Refresh, not the list, is the thing measured: the ABSOLUTE band positions are what the hit-test
// consumes, and the shift alone is weak evidence because every term of it cancels.
const probe = document.getElementById("probe")!;
let firstRefreshTop: number | null = null;
let firstListTop: number | null = null;

const topOf = (sel: string): number | null => {
  const el = document.querySelector(sel);
  return el ? el.getBoundingClientRect().top : null;
};
const refreshTop = () => topOf('[data-testid="refresh-btn"]');
const listTop = () => topOf('[data-testid="checkpoint-list"]');

function render() {
  const now = refreshTop();
  const list = listTop();
  const lines = [
    `CPE-1983  ${legacy ? "BEFORE (max-height: 30vh, no height)" : "AFTER (height: clamp(160px,30vh,260px))"}`,
    `list=${listSize} (${checkpoints.length} checkpoints)  delay=${delayMs}ms  viewport=${window.innerWidth}x${window.innerHeight}`,
    `Refresh top @t=100ms : ${firstRefreshTop === null ? "…" : `${firstRefreshTop.toFixed(1)}px`}`,
    `Refresh top now      : ${now === null ? "n/a" : `${now.toFixed(1)}px`}   ${listResolved ? "(list landed)" : "(in flight)"}`,
    `Refresh moved        : ${firstRefreshTop === null || now === null ? "…" : `${(firstRefreshTop - now).toFixed(1)}px`}`,
    `.list top @t=100ms   : ${firstListTop === null ? "…" : `${firstListTop.toFixed(1)}px`}`,
    `.list top now        : ${list === null ? "n/a" : `${list.toFixed(1)}px`}`,
    // The hit-test in one line: the point the pointer held is `firstRefreshTop`, and if `.list` has
    // climbed above it, that point is now inside the box that carries `Revert…`.
    `click aimed at Refresh now over : ${
      firstRefreshTop === null || list === null ? "…" : firstRefreshTop >= list ? "**.list (Revert…)**" : "Refresh"
    }`,
  ];
  probe.textContent = lines.join("\n");
}

setTimeout(() => {
  firstRefreshTop = refreshTop();
  firstListTop = listTop();
  render();
}, 100);
setInterval(render, 100);
render();
