/** Clamp a panel width to a safe [min, max] range. */
export function clampWidth(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}
