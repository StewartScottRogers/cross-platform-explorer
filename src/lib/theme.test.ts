import { describe, it, expect, afterEach } from "vitest";
import { resolveTheme, applyTheme } from "./theme";

// CPE-1535 (foundation slice of epic CPE-1492): only "light" is real today — "system" resolves to
// "light" too, since there is no dark palette yet (CPE-1493). applyTheme's only observable effect is
// stamping the dataset attribute; the CSS that reacts to it is CPE-1534's exclusive scope, so these
// tests assert the attribute is SET, never a computed colour.
describe("resolveTheme (CPE-1535)", () => {
  it('resolves "system" to "light"', () => {
    expect(resolveTheme("system")).toBe("light");
  });

  it('resolves "light" to "light"', () => {
    expect(resolveTheme("light")).toBe("light");
  });
});

describe("applyTheme (CPE-1535)", () => {
  afterEach(() => {
    delete document.documentElement.dataset.theme;
  });

  it('sets documentElement.dataset.theme to "light" for "system"', () => {
    applyTheme("system");
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it('sets documentElement.dataset.theme to "light" for "light"', () => {
    applyTheme("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});
