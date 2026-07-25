import { describe, it, expect } from "vitest";
import {
  modeToSymbolic,
  modeToOctal,
  octalToMode,
  symbolicToMode,
  describePermissions,
  setPermission,
} from "./permissions";

describe("permissions (CPE-784)", () => {
  it("formats symbolic + octal for common modes", () => {
    expect(modeToSymbolic(0o755)).toBe("rwxr-xr-x");
    expect(modeToSymbolic(0o644)).toBe("rw-r--r--");
    expect(modeToSymbolic(0o000)).toBe("---------");
    expect(modeToSymbolic(0o777)).toBe("rwxrwxrwx");
    expect(modeToOctal(0o755)).toBe("755");
    expect(modeToOctal(0o6)).toBe("006");
    expect(modeToOctal(0o4755)).toBe("755"); // masks to low 9 bits
  });

  it("parses octal + symbolic, rejecting malformed input", () => {
    expect(octalToMode("755")).toBe(0o755);
    expect(octalToMode("0644")).toBe(0o644);
    expect(octalToMode("799")).toBeNull();
    expect(octalToMode("75")).toBeNull();
    expect(symbolicToMode("rwxr-xr-x")).toBe(0o755);
    expect(symbolicToMode("rw-r--r--")).toBe(0o644);
    expect(symbolicToMode("rwxr-xr-")).toBeNull(); // too short
    expect(symbolicToMode("rwxr-xr-z")).toBeNull(); // bad char
  });

  it("round-trips both representations", () => {
    for (const m of [0o755, 0o644, 0o600, 0o777, 0o000, 0o751]) {
      expect(octalToMode(modeToOctal(m))).toBe(m);
      expect(symbolicToMode(modeToSymbolic(m))).toBe(m);
    }
  });

  it("describes per-class read/write/execute", () => {
    const d = describePermissions(0o640);
    expect(d.owner).toEqual({ read: true, write: true, execute: false });
    expect(d.group).toEqual({ read: true, write: false, execute: false });
    expect(d.other).toEqual({ read: false, write: false, execute: false });
  });

  it("toggles a single class/permission bit", () => {
    expect(modeToOctal(setPermission(0o644, "other", "execute", true))).toBe("645");
    expect(modeToOctal(setPermission(0o755, "group", "write", true))).toBe("775");
    expect(modeToOctal(setPermission(0o777, "owner", "read", false))).toBe("377");
    // setting an already-set bit is a no-op
    expect(modeToOctal(setPermission(0o755, "owner", "read", true))).toBe("755");
  });
});
