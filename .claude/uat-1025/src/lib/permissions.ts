// Pure POSIX permission model (CPE-784, epic CPE-710). Convert a mode between symbolic (`rwxr-xr-x`) and
// octal (`755`) forms and expose a per-class read/write/execute breakdown + bit toggles — no DOM/IO,
// unit-tested — so the attributes editor (CPE-786) is a thin render over a backend chmod. Low 9 bits only
// (setuid/setgid/sticky are out of scope for v1).

export type PermClass = "owner" | "group" | "other";
export type Perm = "read" | "write" | "execute";

export interface PermTriple {
  read: boolean;
  write: boolean;
  execute: boolean;
}
export interface PermBreakdown {
  owner: PermTriple;
  group: PermTriple;
  other: PermTriple;
}

const SHIFT: Record<PermClass, number> = { owner: 6, group: 3, other: 0 };
const BIT: Record<Perm, number> = { read: 4, write: 2, execute: 1 };

/** `rwxr-xr-x` for the low 9 bits. */
export function modeToSymbolic(mode: number): string {
  const rwx = (bits: number) => (bits & 4 ? "r" : "-") + (bits & 2 ? "w" : "-") + (bits & 1 ? "x" : "-");
  return rwx((mode >> 6) & 7) + rwx((mode >> 3) & 7) + rwx(mode & 7);
}

/** Three-digit octal, e.g. `"755"`. */
export function modeToOctal(mode: number): string {
  return (mode & 0o777).toString(8).padStart(3, "0");
}

/** Parse a 3- or 4-digit octal string to a mode, or `null` if malformed. */
export function octalToMode(octal: string): number | null {
  if (!/^[0-7]{3,4}$/.test(octal)) return null;
  return parseInt(octal, 8) & 0o777;
}

/** Parse a 9-char `rwxr-xr-x` string to a mode, or `null` if malformed. */
export function symbolicToMode(sym: string): number | null {
  if (!/^[r-][w-][x-][r-][w-][x-][r-][w-][x-]$/.test(sym)) return null;
  let mode = 0;
  const classes: PermClass[] = ["owner", "group", "other"];
  for (let g = 0; g < 3; g++) {
    const base = g * 3;
    let bits = 0;
    if (sym[base] === "r") bits |= 4;
    if (sym[base + 1] === "w") bits |= 2;
    if (sym[base + 2] === "x") bits |= 1;
    mode |= bits << SHIFT[classes[g]];
  }
  return mode;
}

/** Break a mode into per-class read/write/execute flags. */
export function describePermissions(mode: number): PermBreakdown {
  const triple = (bits: number): PermTriple => ({
    read: !!(bits & 4),
    write: !!(bits & 2),
    execute: !!(bits & 1),
  });
  return {
    owner: triple((mode >> 6) & 7),
    group: triple((mode >> 3) & 7),
    other: triple(mode & 7),
  };
}

/** Return a new mode with one class/permission bit set or cleared. */
export function setPermission(mode: number, who: PermClass, perm: Perm, value: boolean): number {
  const bit = BIT[perm] << SHIFT[who];
  return (value ? mode | bit : mode & ~bit) & 0o777;
}
