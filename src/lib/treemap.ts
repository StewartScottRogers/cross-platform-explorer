/**
 * Squarified treemap layout (CPE-751, epic CPE-706) — pure geometry, no DOM.
 *
 * Given items with sizes and a rectangle, lay them out as tiles whose areas are proportional to their
 * sizes, keeping each tile as close to square as possible (the Bruls/Huizing/van Wijk "squarified"
 * algorithm). Deterministic and headless-testable; the `DiskSpaceView` renders the returned tiles as
 * SVG rects. Zero/negative sizes are dropped; a degenerate rect yields no tiles.
 */

export interface TreemapItem {
  /** Opaque id the caller maps back to its data (e.g. the child path). */
  key: string;
  size: number;
}

export interface Tile {
  key: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The worst (largest) aspect ratio among a row of cell areas laid across `side` (thickness = sum/side). */
function worstRatio(areas: number[], sum: number, side: number): number {
  if (sum <= 0 || side <= 0) return Infinity;
  const thickness = sum / side;
  let worst = 0;
  for (const a of areas) {
    if (a <= 0) continue;
    const cellLen = a / thickness;
    const ratio = Math.max(thickness / cellLen, cellLen / thickness);
    if (ratio > worst) worst = ratio;
  }
  return worst;
}

/**
 * Lay `items` into the rectangle at (`x0`,`y0`) of size `w`×`h`. Returns one tile per positive-size
 * item; tiles fully tile the rectangle without overlap (up to floating-point error).
 */
export function squarify(
  items: TreemapItem[],
  x0: number,
  y0: number,
  w: number,
  h: number,
): Tile[] {
  const positive = items.filter((it) => it.size > 0);
  if (positive.length === 0 || w <= 0 || h <= 0) return [];

  const total = positive.reduce((s, it) => s + it.size, 0);
  const scale = (w * h) / total; // size units -> area units
  // Largest first gives the best-looking squarification.
  const scaled = positive
    .map((it) => ({ key: it.key, area: it.size * scale }))
    .sort((a, b) => b.area - a.area);

  const tiles: Tile[] = [];
  let rx = x0;
  let ry = y0;
  let rw = w;
  let rh = h;

  let i = 0;
  while (i < scaled.length) {
    const side = Math.min(rw, rh);
    // Greedily grow a row while it doesn't worsen the worst aspect ratio.
    const rowAreas: number[] = [];
    const rowItems: { key: string; area: number }[] = [];
    let rowSum = 0;
    while (i < scaled.length) {
      const cand = scaled[i];
      const withCand = worstRatio([...rowAreas, cand.area], rowSum + cand.area, side);
      const without = rowItems.length ? worstRatio(rowAreas, rowSum, side) : Infinity;
      if (rowItems.length === 0 || withCand <= without) {
        rowItems.push(cand);
        rowAreas.push(cand.area);
        rowSum += cand.area;
        i++;
      } else {
        break;
      }
    }

    // Lay the finalized row across the short side; the row's thickness eats into the long side.
    const thickness = rowSum / side;
    if (rw >= rh) {
      let cy = ry;
      for (const cell of rowItems) {
        const cellH = cell.area / thickness;
        tiles.push({ key: cell.key, x: rx, y: cy, w: thickness, h: cellH });
        cy += cellH;
      }
      rx += thickness;
      rw -= thickness;
    } else {
      let cx = rx;
      for (const cell of rowItems) {
        const cellW = cell.area / thickness;
        tiles.push({ key: cell.key, x: cx, y: ry, w: cellW, h: thickness });
        cx += cellW;
      }
      ry += thickness;
      rh -= thickness;
    }
  }
  return tiles;
}
