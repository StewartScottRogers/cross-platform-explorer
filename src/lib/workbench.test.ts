// CPE-527: workbench browser URL validation.
import { describe, it, expect } from "vitest";
import { normalizeUrl, isBrowsableUrl } from "./workbench";

describe("workbench browser URL validation (CPE-527)", () => {
  it("normalizes bare localhost / host / IP to http://", () => {
    expect(normalizeUrl("localhost:3000")).toBe("http://localhost:3000");
    expect(normalizeUrl("127.0.0.1:8080")).toBe("http://127.0.0.1:8080");
    expect(normalizeUrl("example.com/path")).toBe("http://example.com/path");
  });

  it("leaves an existing scheme untouched", () => {
    expect(normalizeUrl("https://example.com")).toBe("https://example.com");
    expect(normalizeUrl("http://localhost:5173")).toBe("http://localhost:5173");
  });

  it("accepts only http/https after normalization", () => {
    expect(isBrowsableUrl("localhost:3000")).toBe(true);
    expect(isBrowsableUrl("https://example.com")).toBe(true);
    expect(isBrowsableUrl("http://127.0.0.1:8080/app")).toBe(true);
  });

  it("rejects non-web schemes and junk", () => {
    expect(isBrowsableUrl("file:///etc/passwd")).toBe(false);
    expect(isBrowsableUrl("javascript:alert(1)")).toBe(false);
    expect(isBrowsableUrl("ftp://host")).toBe(false);
    expect(isBrowsableUrl("")).toBe(false);
    expect(isBrowsableUrl("   ")).toBe(false);
  });
});
