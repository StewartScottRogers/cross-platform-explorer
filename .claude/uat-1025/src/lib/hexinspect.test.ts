import { describe, it, expect } from "vitest";
import { inspect, type InspectRow } from "./hexinspect";

const bytes = (...n: number[]) => Uint8Array.from(n);
const val = (rows: InspectRow[], type: string) => rows.find((r) => r.type === type)?.value;

describe("inspect (CPE-771)", () => {
  it("decodes little-endian integers", () => {
    const r = inspect(bytes(0x01, 0x00, 0x00, 0x00), 0, true);
    expect(val(r, "int32")).toBe("1");
    expect(val(r, "uint32")).toBe("1");
    expect(val(r, "int16")).toBe("1");
  });

  it("decodes big-endian integers (same bytes, different value)", () => {
    const r = inspect(bytes(0x01, 0x00, 0x00, 0x00), 0, false);
    expect(val(r, "int32")).toBe(String(0x01000000));
    expect(val(r, "uint16")).toBe(String(0x0100));
  });

  it("decodes signed int8 / uint8", () => {
    const r = inspect(bytes(0xff), 0, true);
    expect(val(r, "int8")).toBe("-1");
    expect(val(r, "uint8")).toBe("255");
  });

  it("decodes float32 = 1.0 in both endiannesses", () => {
    expect(val(inspect(bytes(0x00, 0x00, 0x80, 0x3f), 0, true), "float32")).toBe("1");
    expect(val(inspect(bytes(0x3f, 0x80, 0x00, 0x00), 0, false), "float32")).toBe("1");
  });

  it("decodes 64-bit ints as BigInt strings", () => {
    const r = inspect(bytes(0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff), 0, true);
    expect(val(r, "int64")).toBe("-1");
    expect(val(r, "uint64")).toBe("18446744073709551615");
  });

  it("previews an ASCII string, stopping at the first non-printable byte", () => {
    const r = inspect(bytes(0x48, 0x69, 0x21, 0x00, 0x41), 0, true); // "Hi!" then NUL
    expect(val(r, "string")).toBe("Hi!");
  });

  it("decodes a Unix32 timestamp (epoch → 1970)", () => {
    const r = inspect(bytes(0x00, 0x00, 0x00, 0x00), 0, true);
    expect(val(r, "unix32")).toBe("1970-01-01T00:00:00.000Z");
    // 2^31-ish: 0x5F5E0100 LE bytes -> a 2020-era date; just assert it parses to an ISO string
    const r2 = inspect(bytes(0x00, 0x01, 0x5e, 0x5f), 0, true);
    expect(val(r2, "unix32")).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it("decodes a Windows FILETIME (epoch-diff → 1970)", () => {
    // FILETIME 116444736000000000 (0x019DB1DED53E8000) == Unix epoch
    const le = bytes(0x00, 0x80, 0x3e, 0xd5, 0xde, 0xb1, 0x9d, 0x01);
    expect(val(inspect(le, 0, true), "FILETIME")).toBe("1970-01-01T00:00:00.000Z");
  });

  it("omits rows that would read past the buffer end", () => {
    const r = inspect(bytes(0x41, 0x42, 0x43), 1, true); // 2 bytes remain from offset 1
    expect(val(r, "int8")).toBe("66"); // 'B'
    expect(val(r, "int16")).toBeDefined();
    expect(val(r, "int32")).toBeUndefined(); // needs 4
    expect(val(r, "int64")).toBeUndefined();
    // last byte: only 8/16? offset 2 -> 1 byte
    const last = inspect(bytes(0x41, 0x42, 0x43), 2, true);
    expect(val(last, "int8")).toBe("67");
    expect(val(last, "int16")).toBeUndefined();
  });

  it("returns [] for an out-of-range or non-integer offset", () => {
    expect(inspect(bytes(1, 2, 3), 3, true)).toEqual([]);
    expect(inspect(bytes(1, 2, 3), -1, true)).toEqual([]);
    expect(inspect(bytes(), 0, true)).toEqual([]);
  });

  it("works when the view is a subarray (respects byteOffset)", () => {
    const full = bytes(0xaa, 0xbb, 0x01, 0x00, 0x00, 0x00);
    const sub = full.subarray(2); // starts at the 0x01
    expect(val(inspect(sub, 0, true), "uint32")).toBe("1");
  });
});
