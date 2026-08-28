// CPE-1968 layout harness — mounts the REAL OrganizeDialog.svelte against a canned `organize_plan`,
// so the dialog's reflow (or, after the fix, its absence) can be measured and photographed by a real
// layout engine. See index.html's header for the query parameters and for why this page exists at
// all; see src/lib/components/OrganizeDialog.svelte's `.preview` comment for the defect itself.
import { registerCommand } from "../layout-guard/shared-mocks/bindings.gen";
import OrganizeDialog from "../../../src/lib/components/OrganizeDialog.svelte";

interface Proposal {
  name: string;
  target_subdir: string;
}

const params = new URLSearchParams(location.search);
document.documentElement.dataset.theme = params.get("theme") === "dark" ? "dark" : "light";

const planSize = params.get("plan") ?? "two";
const delayMs = Number(params.get("delay") ?? "120");
const legacy = params.get("legacy") === "1";

/**
 * The PRE-CPE-1968 `.preview` height, re-applied over the shipped one so the "before" screenshots
 * come out of the same working tree as the "after" ones. `!important` because Svelte's scoped class
 * (`.preview.svelte-xxxx`) outranks a bare `.preview` selector from outside the component — this is a
 * deliberate override, not a style the app has.
 */
if (legacy) {
  const style = document.createElement("style");
  style.textContent = `[data-testid="preview"] {
    height: auto !important;
    min-height: 120px !important;
    max-height: 45vh !important;
  }`;
  document.head.append(style);
}

/** Fixtures. `two` is the "is this box absurdly empty?" case the Visual Critic is being asked about;
 *  `large` is an ordinary Downloads folder, which overflows any box this dialog could reasonably have
 *  and is therefore what the scroll viewport is really for. */
const KINDS = ["Images", "Documents", "Archives", "Audio", "Video"];
const PLANS: Record<string, Proposal[]> = {
  none: [],
  two: [
    { name: "CPE-1143-photo.png", target_subdir: "Images" },
    { name: "quarterly-report.pdf", target_subdir: "Documents" },
  ],
  large: Array.from({ length: 26 }, (_, i) => ({
    name: `${["screenshot", "invoice", "backup", "podcast", "clip"][i % 5]}-2026-0${(i % 9) + 1}-${String(i + 3).padStart(2, "0")}.${["png", "pdf", "zip", "mp3", "mp4"][i % 5]}`,
    target_subdir: KINDS[i % KINDS.length],
  })),
};

const plan = PLANS[planSize] ?? PLANS.two;

// `planResolved` is recorded HERE, at the mock, rather than inferred from the DOM. The obvious
// inference — "does [data-testid='summary'] exist yet?" — is wrong for an EMPTY plan, which never
// renders a summary, and it reported `(loading)` on the `empty` screenshot even though the plan had
// landed. And the other candidate, `empty-state`, renders at MOUNT (CPE-1965: `loading` starts false
// and `plan` starts `[]`), so it cannot distinguish the two either. The mock is the only place that
// knows for certain.
let planResolved = false;
registerCommand("organizePlan", async () => {
  await new Promise((r) => setTimeout(r, delayMs));
  planResolved = true;
  return plan;
});

new OrganizeDialog({ target: document.getElementById("mount")!, props: { path: "/home/user/Downloads" } });

// The probe. Each screenshot carries its own measurement of where the rule pills actually are, so a
// before/after pair is a pair of NUMBERS rather than two pictures to eyeball. `firstTop` is sampled
// inside the dialog's own 120ms debounce (the window CPE-1965 measured the swallowed clicks in), and
// the delta is what the pills moved by once the plan landed.
const probe = document.getElementById("probe")!;
let firstTop: number | null = null;

function rulesTop(): number | null {
  const el = document.querySelector('[data-testid="rule-picker"]');
  return el ? el.getBoundingClientRect().top : null;
}

function render() {
  const now = rulesTop();
  const settled = planResolved;
  const lines = [
    `CPE-1968  ${legacy ? "BEFORE (min-height:120px/max-height:45vh)" : "AFTER (height: clamp(200px,40vh,340px))"}`,
    `plan=${planSize} (${plan.length} files)  delay=${delayMs}ms  viewport=${window.innerWidth}x${window.innerHeight}`,
    `.rules top @t=100ms : ${firstTop === null ? "…" : `${firstTop.toFixed(1)}px`}`,
    `.rules top now      : ${now === null ? "n/a" : `${now.toFixed(1)}px`}   ${settled ? "(plan landed)" : "(in flight)"}`,
    `pills moved         : ${firstTop === null || now === null ? "…" : `${(firstTop - now).toFixed(1)}px`}`,
  ];
  probe.textContent = lines.join("\n");
}

setTimeout(() => {
  firstTop = rulesTop();
  render();
}, 100);
setInterval(render, 100);
render();
