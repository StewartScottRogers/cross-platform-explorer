import { describe, it, expect } from "vitest";
import { drivesSignature, drivesChanged } from "./driveWatch";
import type { Place } from "./types";

const drive = (path: string): Place => ({ name: path, path, kind: "drive" });

describe("drivesSignature (CPE-1280)", () => {
  it("is order-independent — same members, any order, share a signature", () => {
    const a = [drive("C:\\"), drive("D:\\")];
    const b = [drive("D:\\"), drive("C:\\")];
    expect(drivesSignature(a)).toBe(drivesSignature(b));
  });

  it("is case-insensitive so d:\\ == D:\\", () => {
    expect(drivesSignature([drive("d:\\")])).toBe(drivesSignature([drive("D:\\")]));
  });

  it("differs when a drive is added or removed", () => {
    const one = [drive("C:\\")];
    const two = [drive("C:\\"), drive("E:\\")];
    expect(drivesSignature(one)).not.toBe(drivesSignature(two));
  });
});

describe("drivesChanged (CPE-1280)", () => {
  it("is false when the member set is unchanged (even reordered)", () => {
    expect(drivesChanged([drive("C:\\"), drive("D:\\")], [drive("D:\\"), drive("C:\\")])).toBe(false);
  });

  it("is true when a USB drive plugs in (appears)", () => {
    expect(drivesChanged([drive("C:\\")], [drive("C:\\"), drive("E:\\")])).toBe(true);
  });

  it("is true when a drive unplugs (drops out)", () => {
    expect(drivesChanged([drive("C:\\"), drive("E:\\")], [drive("C:\\")])).toBe(true);
  });
});
