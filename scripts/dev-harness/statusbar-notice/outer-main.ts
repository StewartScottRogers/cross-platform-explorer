// CPE-1660 notice-overflow verification harness — outer shell script. Reads ?w=<px>&notice=<short|long>
// from the URL, sizes the <iframe> to that width, waits for the iframe to report its diagnostics
// (published on `contentWindow` by inner-main.ts — same-origin, direct property access), and writes a
// human-readable summary into #readout so both a screenshot AND a script reading the DOM can confirm the
// ACHIEVED width and the measured `.statusbar` height before trusting anything.
const params = new URLSearchParams(location.search);
const width = Number(params.get("w") ?? "900");
const noticeParam = params.get("notice");
const notice = noticeParam === "long" ? "long" : noticeParam === "none" ? "none" : "short";
// CPE-1859: forwarded verbatim to the inner document; see inner-main.ts for what they toggle.
const git = params.get("git") === "off" ? "off" : "on";
const disk = params.get("disk") === "off" ? "off" : "on";
// CPE-1836: forwarded verbatim — the compound busy-row scenario.
const busy = params.get("busy") === "1" ? "1" : "0";

const frame = document.getElementById("stage-frame") as HTMLIFrameElement;
frame.style.width = `${width}px`;
frame.style.height = "160px";
frame.src = `./inner.html?notice=${notice}&git=${git}&disk=${disk}&busy=${busy}`;

type Rect = { left: number; right: number; width: number };
type Diag = {
  innerWidth: number;
  statusbarRect?: { height: number; width: number };
  noticeLineCount: number; // approximated: statusbar height / a single-line's own height, rounded
  contentRight?: number;
  itemCount?: Rect | null;
  git?: Rect | null;
  disk?: Rect | null;
  // CPE-1836: the full per-child sweep computed inside the iframe (see inner-main.ts) — forwarded
  // verbatim so the OUTER readout (what `--dump-dom` on index.html actually captures) is itself the
  // complete, self-checking record, not just the four legacy fields above.
  allRects?: Record<string, Rect | null>;
  overlapPairs?: string[];
  spillOutside?: string[];
  gitChildOverhangPx?: Record<string, number>;
  gitOverflowPaintProbe?: { x: number; y: number; hitClass: string | null; hitIsGitDescendant: boolean } | null;
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
    `requested width=${width}px notice=${notice} git=${git} disk=${disk}`,
    `iframe achieved innerWidth=${diag.innerWidth}px`,
    diag.statusbarRect
      ? `.statusbar height=${diag.statusbarRect.height.toFixed(1)}px width=${diag.statusbarRect.width.toFixed(1)}px`
      : `.statusbar not found`,
  ];
  // CPE-1859: the anchoring numbers. `diskRightGap` is the whole assertion — how far `.disk`'s right
  // edge sits from the bar's right padding edge. ~0 means anchored right; a large number means it is
  // sitting wherever the previous sibling left it. `diskFromItemCount` is the complementary reading:
  // ~14px (the bar's `gap`) means it is glued to the item count, i.e. the defect.
  if (diag.contentRight !== undefined) {
    const fmt = (r: Rect | null | undefined) =>
      r ? `left=${r.left.toFixed(1)} right=${r.right.toFixed(1)} w=${r.width.toFixed(1)}` : "ABSENT";
    lines.push(`.item-count ${fmt(diag.itemCount)}`);
    lines.push(`.git        ${fmt(diag.git)}`);
    lines.push(`.disk       ${fmt(diag.disk)}`);
    if (diag.disk) {
      lines.push(`diskRightGap=${(diag.contentRight - diag.disk.right).toFixed(1)}px  (0 ⇒ anchored right)`);
      if (diag.itemCount) {
        lines.push(
          `diskFromItemCount=${(diag.disk.left - diag.itemCount.right).toFixed(1)}px  (≈14 ⇒ glued to the count)`,
        );
      }
    }
  }
  // CPE-1836: print every measured child plus the two derived checks. Both arrays must be empty for a
  // clean render — `overlapPairs` catches a sibling painting over a sibling, `spillOutside` catches a
  // child escaping `.statusbar`'s own padding box (the resize-grip is EXPECTED to sit at the padding
  // edge / slightly past it by design — see its own `right: 0; bottom: 0;` CSS — so it is excluded from
  // the "must stay inside padding" spill check by the inner document's own selector set intentionally
  // measuring it, but reported here unfiltered so a human reviewing the dump sees the raw number).
  if (diag.allRects) {
    lines.push("--- all children ---");
    for (const [label, r] of Object.entries(diag.allRects)) {
      lines.push(`${label.padEnd(14)} ${diag.allRects && r ? `left=${r.left.toFixed(1)} right=${r.right.toFixed(1)} w=${r.width.toFixed(1)}` : "ABSENT"}`);
    }
    lines.push(`overlapPairs=${JSON.stringify(diag.overlapPairs ?? [])}`);
    lines.push(`spillOutside=${JSON.stringify(diag.spillOutside ?? [])}`);
    lines.push(`gitChildOverhangPx=${JSON.stringify(diag.gitChildOverhangPx ?? {})}`);
    lines.push(`gitOverflowPaintProbe=${JSON.stringify(diag.gitOverflowPaintProbe ?? null)}`);
  }
  readout.textContent = lines.join("\n");
  (window as unknown as { __harnessDiag?: Diag }).__harnessDiag = diag;
}

render(undefined);
// CPE-1859: two changes, both forced by driving this harness non-interactively with headless Chrome
// (`--headless=new --virtual-time-budget=N --dump-dom`) rather than by hand in a real window.
//
//  1. The poll no longer waits for the iframe's `load` event. Under the virtual-time policy that event
//     is not reliably delivered to the outer frame before the budget expires, and since the poll was
//     STARTED by it, the readout simply froze on "waiting for iframe…" — intermittently at first, then
//     on nearly every run once the dev server was restarted. Polling unconditionally costs nothing
//     (`readDiag` returns undefined until the inner document publishes) and removes the dependency.
//  2. The retry cap was 40, i.e. 2s of polling — ample for a human, but virtual time FAST-FORWARDS
//     timers, so the whole budget could be spent in one tick before the iframe's module graph (fetched
//     over the dev server, real time that virtual time does not compress) had executed. 2000 is still
//     bounded but cannot run out first.
//
// `requestAnimationFrame` was tried in place of `setInterval` and is worse on both counts: the outer
// frame's rAF callbacks do not advance under the virtual-time policy at all.
let tries = 0;
const iv = setInterval(() => {
  const diag = readDiag();
  tries += 1;
  if (diag || tries > 2000) {
    clearInterval(iv);
    render(diag);
  }
}, 50);
