// CPE-1635 narrow-width verification harness — inner iframe document script. Reads ?theme=<light|dark>
// from the URL (width comes from the OUTER page sizing the <iframe> itself — this document's own
// `window.innerWidth` IS the CSS viewport under test, no query param needed for that), sets
// documentElement.dataset.theme the same way src/lib/theme.ts does in the real app (CPE-1534/1539),
// mounts the real CheckpointDialog.svelte, and publishes a diagnostic object on `window` that the outer
// page reads back via same-origin `iframe.contentWindow` access (both documents are served by the same
// harness dev server, so no postMessage plumbing is needed).
import CheckpointDialog from "../../../src/lib/components/CheckpointDialog.svelte";

const params = new URLSearchParams(location.search);
const theme = params.get("theme") === "dark" ? "dark" : "light";
document.documentElement.dataset.theme = theme;

const app = new CheckpointDialog({
  target: document.getElementById("mount")!,
  props: { initialPath: "/example/project" },
});
(window as unknown as { __checkpointDialog?: unknown }).__checkpointDialog = app;

// ?stress=1 — CPE-1635: proves the min-width:0/ellipsis fix actually engages under a title long enough
// to matter (the default "Checkpoint & rollback" never gets close to needing it). Swaps the real h2's
// text for an artificially long one shortly after mount, real DOM only, no component prop for this
// (there isn't one) — a harness-only stress knob, never shipped.
if (params.get("stress") === "1") {
  requestAnimationFrame(() => {
    const h2 = document.querySelector(".dialog h2");
    if (h2) h2.textContent = "Checkpoint & rollback for a very long example project name that keeps going";
  });
}

function computeDiag() {
  const dialog = document.querySelector(".dialog") as HTMLElement | null;
  const headRow = document.querySelector(".head-row") as HTMLElement | null;
  const actions = document.querySelector(".actions") as HTMLElement | null;
  const diag: Record<string, unknown> = {
    innerWidth: window.innerWidth,
    innerHeight: window.innerHeight,
  };
  if (dialog) {
    const dr = dialog.getBoundingClientRect();
    diag.dialogRect = { left: dr.left, right: dr.right, width: dr.width };
    // Overflow past the TRUE viewport edge — the exact symptom the ticket reports ("pushes past the
    // viewport edge, defying the max-width:95vw the dialog declares").
    diag.dialogOverflowsViewport = dr.right > window.innerWidth + 0.5 || dr.left < -0.5;
  }
  if (headRow) {
    const hr = headRow.getBoundingClientRect();
    diag.headRowRect = { left: hr.left, right: hr.right, width: hr.width };
    diag.headRowOverflowsViewport = hr.right > window.innerWidth + 0.5 || hr.left < -0.5;
  }
  if (actions) {
    const ar = actions.getBoundingClientRect();
    diag.actionsRect = { left: ar.left, right: ar.right, width: ar.width };
    diag.actionsOverflowsViewport = ar.right > window.innerWidth + 0.5 || ar.left < -0.5;
  }
  diag.documentScrollWidth = document.documentElement.scrollWidth;
  diag.hasHorizontalOverflow = document.documentElement.scrollWidth > window.innerWidth + 0.5;
  (window as unknown as { __checkpointDiag?: unknown }).__checkpointDiag = diag;
  (window as unknown as { __checkpointDiagReady?: boolean }).__checkpointDiagReady = true;
}

// Only flip `ready` once the async `checkpointList`/`checkpointFailuresList` mock calls have actually
// resolved and the list re-rendered (real rows or the empty-state note) — otherwise the outer page's
// poll loop (which stops at the FIRST truthy `__checkpointDiagReady`) could grab a stale measurement
// taken before any list content existed, silently missing an overflow that only appears once the real
// rows are laid out. Poll internally rather than a fixed setTimeout so this stays correct even if the
// mock's promise resolution timing changes.
let settledTries = 0;
const settleIv = setInterval(() => {
  settledTries += 1;
  const list = document.querySelector('[data-testid="checkpoint-list"]');
  const settled = !!list && (list.querySelector(".cp-row") !== null || list.querySelector(".empty") !== null);
  if (settled || settledTries > 60) {
    clearInterval(settleIv);
    computeDiag();
    requestAnimationFrame(() => requestAnimationFrame(computeDiag));
  }
}, 50);
