import { describe, it, expect, afterEach, vi } from "vitest";
import { resolveTheme, applyTheme, watchSystemTheme } from "./theme";

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
