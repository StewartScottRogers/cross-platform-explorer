import { describe, it, expect, afterEach, vi } from "vitest";
import { resolveTheme, resolveContrast, applyTheme, watchSystemTheme } from "./theme";

// Minimal mockable shape of the bits of MediaQueryList this module touches.
interface MockMql {
  matches: boolean;
  addEventListener: ReturnType<typeof vi.fn>;
  removeEventListener: ReturnType<typeof vi.fn>;
}

function mockMatchMedia(matches: boolean): MockMql {
  const mql: MockMql = {
    matches,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  };
  window.matchMedia = vi.fn().mockReturnValue(mql) as unknown as typeof window.matchMedia;
  return mql;
}

// CPE-1535 (foundation) + CPE-1540 (system/dark resolution + live watch, epic CPE-1492): "light" and
// "dark" are unconditional overrides; "system" resolves live against
// `window.matchMedia("(prefers-color-scheme: dark)")`, guarded to "light" when matchMedia is
// unavailable (older/non-browser test contexts). applyTheme's only observable effect is stamping the
// dataset attribute; the CSS that reacts to it is CPE-1534/CPE-1539's exclusive scope, so these tests
// assert the attribute is SET, never a computed colour.
describe("resolveTheme (CPE-1535/CPE-1540)", () => {
  const originalMatchMedia = window.matchMedia;

  afterEach(() => {
    window.matchMedia = originalMatchMedia;
  });

  it('resolves "light" to "light" unconditionally', () => {
    mockMatchMedia(true); // even if the OS reports dark, an explicit "light" override wins
    expect(resolveTheme("light")).toBe("light");
  });

  it('resolves "dark" to "dark" unconditionally', () => {
    mockMatchMedia(false); // even if the OS reports light, an explicit "dark" override wins
    expect(resolveTheme("dark")).toBe("dark");
  });

  it('resolves "system" to "dark" when matchMedia reports matches: true', () => {
    mockMatchMedia(true);
    expect(resolveTheme("system")).toBe("dark");
  });

  it('resolves "system" to "light" when matchMedia reports matches: false', () => {
    mockMatchMedia(false);
    expect(resolveTheme("system")).toBe("light");
  });

  it('resolves "system" to "light" when matchMedia is unavailable', () => {
    // @ts-expect-error simulating an environment without matchMedia support (jsdom default, older webviews)
    window.matchMedia = undefined;
    expect(resolveTheme("system")).toBe("light");
  });
});

// CPE-1544 (epic CPE-1496 "high contrast"): resolveContrast resolves the persisted contrast
// preference to whether the high-contrast variant should apply. "high"/"off" are unconditional
// overrides; "system" is the single extension point CPE-1546 plugs the real OS high-contrast read
// into — until then it just returns the `osHighContrastActive` argument, which defaults to `false`.
describe("resolveContrast (CPE-1544)", () => {
  it('resolves "high" to true unconditionally', () => {
    expect(resolveContrast("high")).toBe(true);
    expect(resolveContrast("high", false)).toBe(true);
  });

  it('resolves "off" to false unconditionally', () => {
    expect(resolveContrast("off")).toBe(false);
    expect(resolveContrast("off", true)).toBe(false);
  });

  it('resolves "system" to the osHighContrastActive argument', () => {
    expect(resolveContrast("system", true)).toBe(true);
    expect(resolveContrast("system", false)).toBe(false);
  });

  it('resolves "system" to false when osHighContrastActive is omitted (no OS signal yet, pre-CPE-1546)', () => {
    expect(resolveContrast("system")).toBe(false);
  });
});

describe("applyTheme (CPE-1535/CPE-1540)", () => {
  const originalMatchMedia = window.matchMedia;

  afterEach(() => {
    delete document.documentElement.dataset.theme;
    window.matchMedia = originalMatchMedia;
  });

  it('sets documentElement.dataset.theme to "light" for "light"', () => {
    applyTheme("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it('sets documentElement.dataset.theme to "dark" for "dark"', () => {
    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it('sets documentElement.dataset.theme to "dark" for "system" when the OS prefers dark', () => {
    mockMatchMedia(true);
    applyTheme("system");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it('sets documentElement.dataset.theme to "light" for "system" when the OS prefers light', () => {
    mockMatchMedia(false);
    applyTheme("system");
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  // CPE-1544: applyTheme now optionally composes an orthogonal contrast axis, stamping
  // `hc-${base}` when contrast resolves to high. The bare single-argument call (default
  // contrastPref "off") must keep stamping the plain base value — proving the widened signature is
  // fully backward compatible with every pre-CPE-1544 call site.
  it('a bare applyTheme("dark") still stamps "dark" (default contrastPref "off" is backward compatible)', () => {
    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it('composes "hc-dark" for applyTheme("dark", "high")', () => {
    applyTheme("dark", "high");
    expect(document.documentElement.dataset.theme).toBe("hc-dark");
  });

  it('composes "hc-light" for applyTheme("light", "high")', () => {
    applyTheme("light", "high");
    expect(document.documentElement.dataset.theme).toBe("hc-light");
  });

  it('composes "hc-light" for applyTheme("light", "system", true) (system contrast, OS signal active)', () => {
    applyTheme("light", "system", true);
    expect(document.documentElement.dataset.theme).toBe("hc-light");
  });

  it('composes "light" (no hc- prefix) for applyTheme("light", "system", false) (system contrast, no OS signal)', () => {
    applyTheme("light", "system", false);
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it('stamps the plain base for applyTheme("dark", "off") (explicit contrast off)', () => {
    applyTheme("dark", "off");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});

describe("watchSystemTheme (CPE-1540)", () => {
  const originalMatchMedia = window.matchMedia;

  afterEach(() => {
    window.matchMedia = originalMatchMedia;
  });

  it("fires the callback when matchMedia dispatches a change event", () => {
    const mql = mockMatchMedia(false);
    const onChange = vi.fn();
    watchSystemTheme(onChange);

    expect(mql.addEventListener).toHaveBeenCalledWith("change", expect.any(Function));
    const listener = mql.addEventListener.mock.calls[0][1] as () => void;
    listener();

    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("returns an unsubscribe function that detaches the listener", () => {
    const mql = mockMatchMedia(false);
    const onChange = vi.fn();
    const unsubscribe = watchSystemTheme(onChange);

    unsubscribe();

    expect(mql.removeEventListener).toHaveBeenCalledWith("change", expect.any(Function));
  });

  it("is a safe no-op when matchMedia is unavailable", () => {
    // @ts-expect-error simulating an environment without matchMedia support
    window.matchMedia = undefined;
    const onChange = vi.fn();

    const unsubscribe = watchSystemTheme(onChange);

    expect(() => unsubscribe()).not.toThrow();
    expect(onChange).not.toHaveBeenCalled();
  });
});
