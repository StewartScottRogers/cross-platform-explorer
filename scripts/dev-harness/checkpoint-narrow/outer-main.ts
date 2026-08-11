// CPE-1635 narrow-width verification harness — outer shell script. Reads ?w=<px>&theme=<light|dark>
// from the URL, sizes the <iframe> to that width (height fixed tall enough to show the header + a
// couple of rows without the OUTER page needing to scroll — the dialog's own `.list` scrolls
// internally), waits for the iframe to report its diagnostics (published on `contentWindow` by
// inner-main.ts — same-origin, so direct property access works, no postMessage needed), and writes a
// human-readable summary into #readout so both a screenshot AND a --dump-dom text extraction can
// confirm the ACHIEVED width (via the iframe's own `innerWidth`/`getBoundingClientRect`) before
// trusting anything visual.
const params = new URLSearchParams(location.search);
const width = Number(params.get("w") ?? "420");
const theme = params.get("theme") === "dark" ? "dark" : "light";

const frame = document.getElementById("stage-frame") as HTMLIFrameElement;
frame.style.width = `${width}px`;
frame.style.height = "700px";
const stress = params.get("stress") === "1" ? "&stress=1" : "";
frame.src = `./inner.html?theme=${theme}${stress}`;

type Diag = {
  innerWidth: number;
  innerHeight: number;
  dialogRect?: { left: number; right: number; width: number };
  dialogOverflowsViewport?: boolean;
  headRowRect?: { left: number; right: number; width: number };
  headRowOverflowsViewport?: boolean;
  actionsRect?: { left: number; right: number; width: number };
  actionsOverflowsViewport?: boolean;
  documentScrollWidth: number;
  hasHorizontalOverflow: boolean;
};

function readDiag(): Diag | undefined {
  const w = frame.contentWindow as (Window & { __checkpointDiag?: Diag; __checkpointDiagReady?: boolean }) | null;
  if (!w || !w.__checkpointDiagReady) return undefined;
  return w.__checkpointDiag;
}

function render(diag: Diag | undefined) {
  const readout = document.getElementById("readout")!;
  if (!diag) {
    readout.textContent = `requested ${width}px, theme=${theme} — waiting for iframe…`;
    return;
  }
  const lines = [
    `requested width=${width}px theme=${theme}`,
    `iframe achieved innerWidth=${diag.innerWidth}px innerHeight=${diag.innerHeight}px`,
    diag.dialogRect
      ? `.dialog left=${diag.dialogRect.left.toFixed(1)} right=${diag.dialogRect.right.toFixed(1)} width=${diag.dialogRect.width.toFixed(1)} OVERFLOWS_VIEWPORT=${diag.dialogOverflowsViewport}`
      : `.dialog not found`,
    diag.headRowRect
      ? `.head-row left=${diag.headRowRect.left.toFixed(1)} right=${diag.headRowRect.right.toFixed(1)} width=${diag.headRowRect.width.toFixed(1)} OVERFLOWS_VIEWPORT=${diag.headRowOverflowsViewport}`
      : `.head-row not found`,
    diag.actionsRect
      ? `.actions(Close) left=${diag.actionsRect.left.toFixed(1)} right=${diag.actionsRect.right.toFixed(1)} width=${diag.actionsRect.width.toFixed(1)} OVERFLOWS_VIEWPORT=${diag.actionsOverflowsViewport}`
      : `.actions not found`,
    `document.scrollWidth=${diag.documentScrollWidth} HAS_HORIZONTAL_OVERFLOW=${diag.hasHorizontalOverflow}`,
  ];
  readout.textContent = lines.join("\n");
  (window as unknown as { __harnessDiag?: Diag }).__harnessDiag = diag;
}

render(undefined);
frame.addEventListener("load", () => {
  // Poll briefly for inner-main.ts's rAF-delayed diagnostic write.
  let tries = 0;
  const iv = setInterval(() => {
    const diag = readDiag();
    tries += 1;
    if (diag || tries > 40) {
      clearInterval(iv);
      render(diag);
    }
  }, 50);
});
