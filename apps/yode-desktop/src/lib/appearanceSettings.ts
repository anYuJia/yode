export type ThemeMode = "light" | "dark" | "system";
export type ReduceMotionMode = "system" | "on" | "off";
export type DiffMarkerMode = "color" | "symbols";
export type AppLanguage = "zh" | "en";

export type RgbColor = {
  r: number;
  g: number;
  b: number;
};

export type AppearanceSettingsState = {
  themeMode: ThemeMode;
  themeName: string;
  accentColor: string;
  backgroundColor: string;
  foregroundColor: string;
  uiFont: string;
  codeFont: string;
  translucentSidebar: boolean;
  contrast: number;
  usePointerCursors: boolean;
  reduceMotion: ReduceMotionMode;
  uiFontSize: number;
  codeFontSize: number;
  appScale: number;
  chatFontSize: number;
  sidebarFontSize: number;
  settingsFontSize: number;
  terminalFontSize: number;
  inspectorFontSize: number;
  diffMarkers: DiffMarkerMode;
  fontSmoothing: boolean;
  pet: string;
};

export type ThemePreset = {
  bg: string;
  fg: string;
  accent: string;
};

export const DEFAULT_UI_FONT = "-apple-system, BlinkMacSystemFont, \"Segoe UI\", system-ui, sans-serif";
export const DEFAULT_CODE_FONT = "ui-monospace, \"SF Mono\", SFMono-Regular, Menlo, Monaco, Consolas, monospace";
export const LANGUAGE_STORAGE_KEY = "yode-language";
export const LANGUAGE_CHANGE_EVENT = "yode-language-change";
export const APPEARANCE_CHANGE_EVENT = "yode-appearance-change";
export const PET_CHANGE_EVENT = "yode-pet-change";
export const DEFAULT_APP_LANGUAGE: AppLanguage = "zh";
export const DEFAULT_PET_NAME = "Yode";

export const DARK_THEME_PRESETS: Record<string, ThemePreset> = {
  Dracula: { bg: "#282A36", fg: "#F8F8F2", accent: "#FF79C6" },
  "One Dark": { bg: "#282C34", fg: "#ABB2BF", accent: "#61AFEF" },
  Nord: { bg: "#2F343F", fg: "#D8DEE9", accent: "#88C0D0" },
  Monokai: { bg: "#272822", fg: "#F8F8F2", accent: "#F92672" },
  Catppuccin: { bg: "#1E1E2E", fg: "#CDD6F4", accent: "#F5C2E7" },
  "GitHub Dark": { bg: "#0D1117", fg: "#C9D1D9", accent: "#58A6FF" },
  Solarized: { bg: "#002B36", fg: "#839496", accent: "#268BD2" },
  Gruvbox: { bg: "#282828", fg: "#EBDBB2", accent: "#FE8019" },
  Ayu: { bg: "#0F1419", fg: "#E6B450", accent: "#F29718" },
  "Tokyo Night": { bg: "#1A1B26", fg: "#A9B1D6", accent: "#7AA2F7" },
  Everforest: { bg: "#2D353B", fg: "#D3C6AA", accent: "#A7C080" },
  Linear: { bg: "#121214", fg: "#F7F8F8", accent: "#5E6AD2" }
};

export const LIGHT_THEME_PRESETS: Record<string, ThemePreset> = {
  Dracula: { bg: "#FAFAFA", fg: "#282A36", accent: "#E0007A" },
  "One Dark": { bg: "#F5F5F5", fg: "#282C34", accent: "#007ACC" },
  Nord: { bg: "#ECEFF4", fg: "#2E3440", accent: "#3B82F6" },
  Monokai: { bg: "#FDF6E3", fg: "#272822", accent: "#D33682" },
  Catppuccin: { bg: "#EFF1F5", fg: "#4C4F69", accent: "#EA76CB" },
  "GitHub Dark": { bg: "#FFFFFF", fg: "#24292F", accent: "#0969DA" },
  Solarized: { bg: "#FDF6E3", fg: "#657B83", accent: "#B58900" },
  Gruvbox: { bg: "#FBF1C7", fg: "#3C3836", accent: "#D65D0E" },
  Ayu: { bg: "#FAFAFA", fg: "#5C6773", accent: "#FF9900" },
  "Tokyo Night": { bg: "#F5F6F9", fg: "#373B41", accent: "#4E75EC" },
  Everforest: { bg: "#FDF6E3", fg: "#5C6A72", accent: "#8DA101" },
  Linear: { bg: "#FFFFFF", fg: "#121214", accent: "#5E6AD2" }
};

export function clampNumber(value: number, min: number, max: number) {
  if (!Number.isFinite(value)) return min;
  return Math.min(max, Math.max(min, value));
}

export function loadStoredNumber(key: string, fallback: number, min = Number.NEGATIVE_INFINITY, max = Number.POSITIVE_INFINITY) {
  const raw = localStorage.getItem(key);
  if (raw === null) return fallback;
  return clampNumber(Number(raw), min, max);
}

export function storedOption<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  const raw = localStorage.getItem(key);
  return allowed.includes(raw as T) ? raw as T : fallback;
}

export function normalizeAppLanguage(value: unknown): AppLanguage {
  return value === "en" || value === "zh" ? value : DEFAULT_APP_LANGUAGE;
}

export function languageFromChangeEvent(event: Event): AppLanguage {
  return event instanceof CustomEvent
    ? normalizeAppLanguage(event.detail)
    : DEFAULT_APP_LANGUAGE;
}

export function loadAppLanguage(): AppLanguage {
  return normalizeAppLanguage(localStorage.getItem(LANGUAGE_STORAGE_KEY));
}

export function loadPetName() {
  return localStorage.getItem("yode-pet") || DEFAULT_PET_NAME;
}

export function petFromChangeEvent(event: Event): string {
  if (!(event instanceof CustomEvent)) return loadPetName();
  return typeof event.detail === "string" && event.detail.trim()
    ? event.detail
    : loadPetName();
}

export function dispatchLanguageChange(appLang: AppLanguage) {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(LANGUAGE_CHANGE_EVENT, { detail: appLang }));
  }
}

export function saveAppLanguage(appLang: string) {
  const nextLang = normalizeAppLanguage(appLang);
  localStorage.setItem(LANGUAGE_STORAGE_KEY, nextLang);
  dispatchLanguageChange(nextLang);
  return nextLang;
}

export function dispatchAppearanceChange() {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(APPEARANCE_CHANGE_EVENT));
  }
}

export function dispatchPetChange(pet: string) {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(PET_CHANGE_EVENT, { detail: pet }));
  }
}

export function hexToRgb(hex: string): RgbColor | null {
  const shorthandRegex = /^#?([a-f\d])([a-f\d])([a-f\d])$/i;
  const fullHex = hex.replace(shorthandRegex, (_, r, g, b) => r + r + g + g + b + b);
  const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(fullHex);
  return result
    ? {
        r: parseInt(result[1], 16),
        g: parseInt(result[2], 16),
        b: parseInt(result[3], 16)
      }
    : null;
}

export function rgbToHex(r: number, g: number, b: number) {
  const toHex = (value: number) => {
    const hex = Math.max(0, Math.min(255, value)).toString(16);
    return hex.length === 1 ? "0" + hex : hex;
  };
  return "#" + toHex(r) + toHex(g) + toHex(b);
}

export function isLightColor(hex: string) {
  const rgb = hexToRgb(hex);
  if (!rgb) return false;
  const luminance = 0.299 * rgb.r + 0.587 * rgb.g + 0.114 * rgb.b;
  return luminance > 128;
}

export function adjustBrightness(hex: string, percent: number) {
  const rgb = hexToRgb(hex);
  if (!rgb) return hex;
  const factor = 1 + percent / 100;
  const r = Math.max(0, Math.min(255, Math.round(rgb.r * factor)));
  const g = Math.max(0, Math.min(255, Math.round(rgb.g * factor)));
  const b = Math.max(0, Math.min(255, Math.round(rgb.b * factor)));
  return rgbToHex(r, g, b);
}

export function prefersDarkMode() {
  const matchMedia =
    typeof window !== "undefined"
      ? window.matchMedia
      : (globalThis as typeof globalThis & { matchMedia?: Window["matchMedia"] }).matchMedia;
  if (!matchMedia) return false;
  return matchMedia("(prefers-color-scheme: dark)").matches;
}

export function resolveThemeIsLight(themeMode: ThemeMode) {
  return themeMode === "light" || (themeMode === "system" && !prefersDarkMode());
}

export function loadAppearanceSettings(): AppearanceSettingsState {
  return {
    themeMode: storedOption("yode-theme-mode", ["light", "dark", "system"] as const, "dark"),
    themeName: localStorage.getItem("yode-theme-name") || "Dracula",
    accentColor: localStorage.getItem("yode-accent-color") || "#FF79C6",
    backgroundColor: localStorage.getItem("yode-bg-color") || "#282A36",
    foregroundColor: localStorage.getItem("yode-fg-color") || "#F8F8F2",
    uiFont: localStorage.getItem("yode-ui-font") || DEFAULT_UI_FONT,
    codeFont: localStorage.getItem("yode-code-font") || DEFAULT_CODE_FONT,
    translucentSidebar: localStorage.getItem("yode-translucent-sidebar") !== "false",
    contrast: loadStoredNumber("yode-contrast", 48),
    usePointerCursors: localStorage.getItem("yode-use-pointers") === "true",
    reduceMotion: storedOption("yode-reduce-motion", ["system", "on", "off"] as const, "system"),
    uiFontSize: loadStoredNumber("yode-ui-font-size", 13, 10, 18),
    codeFontSize: loadStoredNumber("yode-code-font-size", 12, 10, 20),
    appScale: loadStoredNumber("yode-app-scale", 100, 85, 130),
    chatFontSize: loadStoredNumber("yode-chat-font-size", 13.25, 11, 20),
    sidebarFontSize: loadStoredNumber("yode-sidebar-font-size", 13, 10, 18),
    settingsFontSize: loadStoredNumber("yode-settings-font-size", 13, 10, 18),
    terminalFontSize: loadStoredNumber("yode-terminal-font-size", 12, 10, 22),
    inspectorFontSize: loadStoredNumber("yode-inspector-font-size", 12, 10, 18),
    diffMarkers: storedOption("yode-diff-markers", ["color", "symbols"] as const, "color"),
    fontSmoothing: localStorage.getItem("yode-font-smoothing") !== "false",
    pet: loadPetName()
  };
}

export function themePresetForMode(themeName: string, themeMode: ThemeMode) {
  const presets = resolveThemeIsLight(themeMode) ? LIGHT_THEME_PRESETS : DARK_THEME_PRESETS;
  return presets[themeName] || presets.Dracula;
}

function scaledPx(value: number, appScale: number) {
  return `${Number((value * (appScale / 100)).toFixed(2))}px`;
}

export function applyAppearanceSettings(settings: AppearanceSettingsState) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const effectiveDark = settings.themeMode === "dark" || (settings.themeMode === "system" && prefersDarkMode());
  root.classList.remove("light", "dark");
  root.classList.add(effectiveDark ? "dark" : "light");
  root.style.setProperty("color-scheme", effectiveDark ? "dark" : "light");

  root.style.setProperty("--accent", settings.accentColor);
  root.style.setProperty("--bg", settings.backgroundColor);
  root.style.setProperty("--text", settings.foregroundColor);
  root.style.setProperty("--font-ui", settings.uiFont);
  root.style.setProperty("--font-code", settings.codeFont);
  root.style.setProperty("--ui-font-size", scaledPx(settings.uiFontSize, settings.appScale));
  root.style.setProperty("--chat-font-size", scaledPx(settings.chatFontSize, settings.appScale));
  root.style.setProperty("--sidebar-font-size", scaledPx(settings.sidebarFontSize, settings.appScale));
  root.style.setProperty("--settings-font-size", scaledPx(settings.settingsFontSize, settings.appScale));
  root.style.setProperty("--code-font-size", scaledPx(settings.codeFontSize, settings.appScale));
  root.style.setProperty("--terminal-font-size", scaledPx(settings.terminalFontSize, settings.appScale));
  root.style.setProperty("--inspector-font-size", scaledPx(settings.inspectorFontSize, settings.appScale));
  root.style.setProperty("--app-scale", String(settings.appScale / 100));
  root.style.setProperty("--contrast-val", String(settings.contrast));
  root.style.fontSize = scaledPx(settings.uiFontSize, settings.appScale);

  const light = isLightColor(settings.backgroundColor);
  const bgPercentMod = light ? -5 : 5;
  const bgDoubleMod = light ? -10 : 10;
  const bgTripleMod = light ? -15 : 15;
  const borderMod = light ? -18 : 18;
  const borderSoftMod = light ? -10 : 10;
  const rgbAccent = hexToRgb(settings.accentColor);

  root.style.setProperty("--chrome", adjustBrightness(settings.backgroundColor, bgPercentMod));
  root.style.setProperty("--panel", adjustBrightness(settings.backgroundColor, bgDoubleMod));
  root.style.setProperty("--panel-raised", adjustBrightness(settings.backgroundColor, bgTripleMod));
  root.style.setProperty("--field", adjustBrightness(settings.backgroundColor, bgPercentMod));
  root.style.setProperty("--line", adjustBrightness(settings.backgroundColor, borderMod));
  root.style.setProperty("--line-soft", adjustBrightness(settings.backgroundColor, borderSoftMod));
  root.style.setProperty(
    "--accent-muted",
    rgbAccent ? `rgba(${rgbAccent.r}, ${rgbAccent.g}, ${rgbAccent.b}, 0.2)` : "rgba(255, 255, 255, 0.1)"
  );

  document.body.classList.toggle("use-pointers", settings.usePointerCursors);
  document.body.classList.remove("reduce-motion");
  const prefersReducedMotion =
    typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
  if (settings.reduceMotion === "on" || (settings.reduceMotion === "system" && prefersReducedMotion)) {
    document.body.classList.add("reduce-motion");
  }
  document.body.classList.toggle("font-smoothing", settings.fontSmoothing);
  document.body.classList.toggle("no-font-smoothing", !settings.fontSmoothing);
}

export function applyTranslucentSidebarSetting(translucentSidebar = loadAppearanceSettings().translucentSidebar) {
  if (typeof document === "undefined") return;
  const shells = document.querySelectorAll(".app-shell");
  shells.forEach((shell) => {
    shell.classList.toggle("translucent-sidebar", translucentSidebar);
    shell.classList.toggle("translucent-sidebar-disabled", !translucentSidebar);
  });
}

export function saveAppearanceSettings(settings: AppearanceSettingsState) {
  localStorage.setItem("yode-theme-mode", settings.themeMode);
  localStorage.setItem("yode-theme-name", settings.themeName);
  localStorage.setItem("yode-accent-color", settings.accentColor);
  localStorage.setItem("yode-bg-color", settings.backgroundColor);
  localStorage.setItem("yode-fg-color", settings.foregroundColor);
  localStorage.setItem("yode-ui-font", settings.uiFont);
  localStorage.setItem("yode-code-font", settings.codeFont);
  localStorage.setItem("yode-translucent-sidebar", String(settings.translucentSidebar));
  localStorage.setItem("yode-contrast", String(settings.contrast));
  localStorage.setItem("yode-use-pointers", String(settings.usePointerCursors));
  localStorage.setItem("yode-reduce-motion", settings.reduceMotion);
  localStorage.setItem("yode-ui-font-size", String(settings.uiFontSize));
  localStorage.setItem("yode-code-font-size", String(settings.codeFontSize));
  localStorage.setItem("yode-app-scale", String(settings.appScale));
  localStorage.setItem("yode-chat-font-size", String(settings.chatFontSize));
  localStorage.setItem("yode-sidebar-font-size", String(settings.sidebarFontSize));
  localStorage.setItem("yode-settings-font-size", String(settings.settingsFontSize));
  localStorage.setItem("yode-terminal-font-size", String(settings.terminalFontSize));
  localStorage.setItem("yode-inspector-font-size", String(settings.inspectorFontSize));
  localStorage.setItem("yode-diff-markers", settings.diffMarkers);
  localStorage.setItem("yode-font-smoothing", String(settings.fontSmoothing));
  localStorage.setItem("yode-pet", settings.pet);
}

export function applyStoredAppearanceSettings() {
  const settings = loadAppearanceSettings();
  applyAppearanceSettings(settings);
  return settings;
}
