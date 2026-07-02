import { afterEach, describe, expect, it, vi } from "vitest";

import {
  DEFAULT_CODE_FONT,
  DEFAULT_UI_FONT,
  loadAppearanceSettings,
  themePresetForMode
} from "./appearanceSettings";

describe("appearance settings helpers", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("loads defaults from local storage", () => {
    stubMemoryLocalStorage();

    expect(loadAppearanceSettings()).toMatchObject({
      themeMode: "dark",
      themeName: "Dracula",
      accentColor: "#FF79C6",
      backgroundColor: "#282A36",
      foregroundColor: "#F8F8F2",
      uiFont: DEFAULT_UI_FONT,
      codeFont: DEFAULT_CODE_FONT,
      translucentSidebar: true,
      reduceMotion: "system",
      fontSmoothing: true
    });
  });

  it("selects light presets in system light mode", () => {
    stubMemoryLocalStorage();
    stubMatchMedia(false);

    expect(themePresetForMode("Dracula", "system")).toMatchObject({
      bg: "#FAFAFA",
      fg: "#282A36"
    });
  });

  it("selects dark presets in system dark mode", () => {
    stubMemoryLocalStorage();
    stubMatchMedia(true);

    expect(themePresetForMode("Dracula", "system")).toMatchObject({
      bg: "#282A36",
      fg: "#F8F8F2"
    });
  });
});

function stubMemoryLocalStorage(seed: Record<string, string> = {}) {
  const values = new Map(Object.entries(seed));
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
    removeItem: (key: string) => {
      values.delete(key);
    }
  });
}

function stubMatchMedia(matches: boolean) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn()
  }));
}
