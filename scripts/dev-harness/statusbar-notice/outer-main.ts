// CPE-1660 notice-overflow verification harness — outer shell script. Reads ?w=<px>&notice=<short|long>
// from the URL, sizes the <iframe> to that width, waits for the iframe to report its diagnostics
// (published on `contentWindow` by inner-main.ts — same-origin, direct property access), and writes a
// human-readable summary into #readout so both a screenshot AND a script reading the DOM can confirm the
// ACHIEVED width and the measured `.statusbar` height before trusting anything.
const params = new URLSearchParams(location.search);
const width = Number(params.get("w") ?? "900");
const notice = params.get("notice") === "long" ? "long" : "short";

const frame = document.getElementById("stage-frame") as HTMLIFrameElement;
frame.style.width = `${width}px`;
frame.style.height = "120px";
frame.src = `./inner.html?notice=${notice}`;

type Diag = {
  innerWidth: number;
  statusbarRect?: { height: number; width: number };
  noticeLineCount: number; // approximated: statusbar height / a single-line's own height, rounded
};

function readDiag(): Diag | undefined {
  const w = frame.contentWindow as (Window & { __sbDiag?: Diag; __sbDiagReady?: boolean }) | null;
  if (!w || !w.__sbDiagReady) return undefined;
  return w.__sbDiag;
}

function render(diag: Diag | undefined) {
  const readout = document.getElementById("readout")!;
  if (!diag) {
    readout.textContent = `requested ${width}px, notice=${notice} — waiting for iframe…`;
    return;
  }
  const lines = [
    `requested width=${width}px notice=${notice}`,
    `iframe achieved innerWidth=${diag.innerWidth}px`,
    diag.statusbarRect
      ? `.statusbar height=${diag.statusbarRect.height.toFixed(1)}px width=${diag.statusbarRect.width.toFixed(1)}px`
      : `.statusbar not found`,
  ];
  readout.textContent = lines.join("\n");
  (window as unknown as { __harnessDiag?: Diag }).__harnessDiag = diag;
}

render(undefined);
frame.addEventListener("load", () => {
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
