import { afterEach, describe, expect, it, vi } from "vitest";

import {
  APPEARANCE_CHANGE_EVENT,
  DEFAULT_CODE_FONT,
  DEFAULT_APP_LANGUAGE,
  DEFAULT_PET_NAME,
  DEFAULT_UI_FONT,
  LANGUAGE_CHANGE_EVENT,
  PET_CHANGE_EVENT,
  dispatchAppearanceChange,
  dispatchPetChange,
  languageFromChangeEvent,
  loadAppLanguage,
  loadAppearanceSettings,
  loadPetName,
  normalizeAppLanguage,
  petFromChangeEvent,
  saveAppLanguage,
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
      fontSmoothing: true,
      pet: DEFAULT_PET_NAME
    });
  });

  it("normalizes and persists app language through one helper", () => {
    const dispatchEvent = vi.fn();
    stubMemoryLocalStorage();
    vi.stubGlobal("window", { dispatchEvent });

    expect(normalizeAppLanguage("fr")).toBe(DEFAULT_APP_LANGUAGE);
    expect(loadAppLanguage()).toBe(DEFAULT_APP_LANGUAGE);
    expect(saveAppLanguage("en")).toBe("en");
    expect(localStorage.getItem("yode-language")).toBe("en");
    expect(dispatchEvent).toHaveBeenCalledWith(expect.objectContaining({
      type: LANGUAGE_CHANGE_EVENT,
      detail: "en"
    }));
    expect(languageFromChangeEvent(new CustomEvent(LANGUAGE_CHANGE_EVENT, { detail: "en" }))).toBe("en");
    expect(languageFromChangeEvent(new CustomEvent(LANGUAGE_CHANGE_EVENT, { detail: "fr" }))).toBe(DEFAULT_APP_LANGUAGE);
    expect(languageFromChangeEvent(new Event(LANGUAGE_CHANGE_EVENT))).toBe(DEFAULT_APP_LANGUAGE);
  });

  it("loads pet names and dispatches appearance events through shared helpers", () => {
    const dispatchEvent = vi.fn();
    stubMemoryLocalStorage({ "yode-pet": "Ada" });
    vi.stubGlobal("window", { dispatchEvent });

    expect(loadPetName()).toBe("Ada");

    dispatchAppearanceChange();
    dispatchPetChange("Yode");

    expect(dispatchEvent).toHaveBeenNthCalledWith(1, expect.objectContaining({ type: APPEARANCE_CHANGE_EVENT }));
    expect(dispatchEvent).toHaveBeenNthCalledWith(2, expect.objectContaining({
      type: PET_CHANGE_EVENT,
      detail: "Yode"
    }));
    expect(petFromChangeEvent(new CustomEvent(PET_CHANGE_EVENT, { detail: "Ada" }))).toBe("Ada");
    expect(petFromChangeEvent(new CustomEvent(PET_CHANGE_EVENT, { detail: "" }))).toBe("Ada");
    expect(petFromChangeEvent(new Event(PET_CHANGE_EVENT))).toBe("Ada");
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
