import React, { useState, useEffect } from "react";
import {
  Sun,
  Moon,
  Monitor,
  Download,
  Copy
} from "lucide-react";
import { CustomSelect } from "../../CustomSelect";
import { ColorPicker } from "../../ColorPicker";

function clampNumber(value: number, min: number, max: number) {
  if (!Number.isFinite(value)) return min;
  return Math.min(max, Math.max(min, value));
}

function loadStoredNumber(key: string, fallback: number, min: number, max: number) {
  const raw = localStorage.getItem(key);
  if (raw === null) return fallback;
  return clampNumber(Number(raw), min, max);
}

function storedOption<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  const raw = localStorage.getItem(key);
  return allowed.includes(raw as T) ? raw as T : fallback;
}

export function AppearanceSettings() {
  const [themeMode, setThemeMode] = useState<"light" | "dark" | "system">(
    () => storedOption("yode-theme-mode", ["light", "dark", "system"] as const, "dark")
  );
  const [themeName, setThemeName] = useState(
    () => localStorage.getItem("yode-theme-name") || "Dracula"
  );
  const [accentColor, setAccentColor] = useState(
    () => localStorage.getItem("yode-accent-color") || "#FF79C6"
  );
  const [backgroundColor, setBackgroundColor] = useState(
    () => localStorage.getItem("yode-bg-color") || "#282A36"
  );
  const [foregroundColor, setForegroundColor] = useState(
    () => localStorage.getItem("yode-fg-color") || "#F8F8F2"
  );
  const [uiFont, setUiFont] = useState(
    () => localStorage.getItem("yode-ui-font") || "-apple-system, BlinkMacSystemFont, \"Segoe UI\", system-ui, sans-serif"
  );
  const [codeFont, setCodeFont] = useState(
    () => localStorage.getItem("yode-code-font") || "ui-monospace, \"SF Mono\", SFMono-Regular, Menlo, Monaco, Consolas, monospace"
  );
  const [translucentSidebar, setTranslucentSidebar] = useState(() => {
    const val = localStorage.getItem("yode-translucent-sidebar");
    return val === null ? true : val === "true";
  });
  const [contrast, setContrast] = useState(() => {
    const val = localStorage.getItem("yode-contrast");
    return val === null ? 48 : Number(val);
  });
  const [usePointerCursors, setUsePointerCursors] = useState(
    () => localStorage.getItem("yode-use-pointers") === "true"
  );
  const [reduceMotion, setReduceMotion] = useState<"system" | "on" | "off">(
    () => storedOption("yode-reduce-motion", ["system", "on", "off"] as const, "system")
  );
  const [uiFontSize, setUiFontSize] = useState(() => {
    return loadStoredNumber("yode-ui-font-size", 13, 10, 18);
  });
  const [codeFontSize, setCodeFontSize] = useState(() => {
    return loadStoredNumber("yode-code-font-size", 12, 10, 20);
  });
  const [appScale, setAppScale] = useState(() => {
    return loadStoredNumber("yode-app-scale", 100, 85, 130);
  });
  const [chatFontSize, setChatFontSize] = useState(() => {
    return loadStoredNumber("yode-chat-font-size", 13.25, 11, 20);
  });
  const [sidebarFontSize, setSidebarFontSize] = useState(() => {
    return loadStoredNumber("yode-sidebar-font-size", 13, 10, 18);
  });
  const [settingsFontSize, setSettingsFontSize] = useState(() => {
    return loadStoredNumber("yode-settings-font-size", 13, 10, 18);
  });
  const [terminalFontSize, setTerminalFontSize] = useState(() => {
    return loadStoredNumber("yode-terminal-font-size", 12, 10, 22);
  });
  const [inspectorFontSize, setInspectorFontSize] = useState(() => {
    return loadStoredNumber("yode-inspector-font-size", 12, 10, 18);
  });
  const [diffMarkers, setDiffMarkers] = useState<"color" | "symbols">(
    () => storedOption("yode-diff-markers", ["color", "symbols"] as const, "color")
  );
  const [fontSmoothing, setFontSmoothing] = useState(() => {
    const val = localStorage.getItem("yode-font-smoothing");
    return val === null ? true : val === "true";
  });
  const [pet, setPet] = useState(
    () => localStorage.getItem("yode-pet") || "Yode"
  );
  const [statusText, setStatusText] = useState("");

  const hexToRgb = (hex: string) => {
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
  };

  const rgbToHex = (r: number, g: number, b: number) => {
    const toHex = (c: number) => {
      const hex = Math.max(0, Math.min(255, c)).toString(16);
      return hex.length === 1 ? "0" + hex : hex;
    };
    return "#" + toHex(r) + toHex(g) + toHex(b);
  };

  const isLightColor = (hex: string) => {
    const rgb = hexToRgb(hex);
    if (!rgb) return false;
    const luminance = 0.299 * rgb.r + 0.587 * rgb.g + 0.114 * rgb.b;
    return luminance > 128;
  };

  const adjustBrightness = (hex: string, percent: number) => {
    const rgb = hexToRgb(hex);
    if (!rgb) return hex;
    const factor = 1 + percent / 100;
    const r = Math.max(0, Math.min(255, Math.round(rgb.r * factor)));
    const g = Math.max(0, Math.min(255, Math.round(rgb.g * factor)));
    const b = Math.max(0, Math.min(255, Math.round(rgb.b * factor)));
    return rgbToHex(r, g, b);
  };

  const presets: Record<string, { bg: string; fg: string; accent: string }> = {
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

  const lightPresets: Record<string, { bg: string; fg: string; accent: string }> = {
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

  const saveItem = (key: string, val: unknown) => {
    localStorage.setItem(key, String(val));
  };

  useEffect(() => {
    const isLight =
      themeMode === "light" ||
      (themeMode === "system" && !window.matchMedia("(prefers-color-scheme: dark)").matches);
    const presetDict = isLight ? lightPresets : presets;
    const preset = presetDict[themeName] || presetDict["Dracula"];
    if (preset) {
      setAccentColor(preset.accent);
      setBackgroundColor(preset.bg);
      setForegroundColor(preset.fg);
      saveItem("yode-theme-name", themeName);
      saveItem("yode-accent-color", preset.accent);
      saveItem("yode-bg-color", preset.bg);
      saveItem("yode-fg-color", preset.fg);
    }
  }, [themeName, themeMode]);

  useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty("--accent", accentColor);
    root.style.setProperty("--bg", backgroundColor);
    root.style.setProperty("--text", foregroundColor);
    root.style.setProperty("--font-ui", uiFont);
    root.style.setProperty("--font-code", codeFont);
    const scale = appScale / 100;
    const scaledPx = (value: number) => `${Number((value * scale).toFixed(2))}px`;
    root.style.setProperty("--ui-font-size", scaledPx(uiFontSize));
    root.style.setProperty("--chat-font-size", scaledPx(chatFontSize));
    root.style.setProperty("--sidebar-font-size", scaledPx(sidebarFontSize));
    root.style.setProperty("--settings-font-size", scaledPx(settingsFontSize));
    root.style.setProperty("--code-font-size", scaledPx(codeFontSize));
    root.style.setProperty("--terminal-font-size", scaledPx(terminalFontSize));
    root.style.setProperty("--inspector-font-size", scaledPx(inspectorFontSize));
    root.style.setProperty("--app-scale", String(scale));
    root.style.setProperty("--contrast-val", String(contrast));
    root.style.fontSize = scaledPx(uiFontSize);

    const light = isLightColor(backgroundColor);
    const bgPercentMod = light ? -5 : 5;
    const bgDoubleMod = light ? -10 : 10;
    const bgTripleMod = light ? -15 : 15;
    const borderMod = light ? -18 : 18;
    const borderSoftMod = light ? -10 : 10;

    const chromeColor = adjustBrightness(backgroundColor, bgPercentMod);
    const panelColor = adjustBrightness(backgroundColor, bgDoubleMod);
    const panelRaised = adjustBrightness(backgroundColor, bgTripleMod);
    const fieldColor = adjustBrightness(backgroundColor, bgPercentMod);
    const lineColor = adjustBrightness(backgroundColor, borderMod);
    const lineSoftColor = adjustBrightness(backgroundColor, borderSoftMod);

    const rgbAccent = hexToRgb(accentColor);
    const accentMuted = rgbAccent
      ? `rgba(${rgbAccent.r}, ${rgbAccent.g}, ${rgbAccent.b}, 0.2)`
      : "rgba(255, 255, 255, 0.1)";

    root.style.setProperty("--chrome", chromeColor);
    root.style.setProperty("--panel", panelColor);
    root.style.setProperty("--panel-raised", panelRaised);
    root.style.setProperty("--field", fieldColor);
    root.style.setProperty("--line", lineColor);
    root.style.setProperty("--line-soft", lineSoftColor);
    root.style.setProperty("--accent-muted", accentMuted);

    saveItem("yode-accent-color", accentColor);
    saveItem("yode-bg-color", backgroundColor);
    saveItem("yode-fg-color", foregroundColor);
    saveItem("yode-ui-font", uiFont);
    saveItem("yode-code-font", codeFont);
    saveItem("yode-code-font-size", codeFontSize);
    saveItem("yode-contrast", contrast);
    saveItem("yode-ui-font-size", uiFontSize);
    saveItem("yode-app-scale", appScale);
    saveItem("yode-chat-font-size", chatFontSize);
    saveItem("yode-sidebar-font-size", sidebarFontSize);
    saveItem("yode-settings-font-size", settingsFontSize);
    saveItem("yode-terminal-font-size", terminalFontSize);
    saveItem("yode-inspector-font-size", inspectorFontSize);
    window.dispatchEvent(new CustomEvent("yode-appearance-change"));
  }, [
    accentColor,
    backgroundColor,
    foregroundColor,
    uiFont,
    codeFont,
    codeFontSize,
    contrast,
    uiFontSize,
    appScale,
    chatFontSize,
    sidebarFontSize,
    settingsFontSize,
    terminalFontSize,
    inspectorFontSize
  ]);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.remove("light", "dark");
    if (themeMode === "light") {
      root.classList.add("light");
      root.style.setProperty("color-scheme", "light");
    } else if (themeMode === "dark") {
      root.classList.add("dark");
      root.style.setProperty("color-scheme", "dark");
    } else {
      const isSystemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
      root.classList.add(isSystemDark ? "dark" : "light");
      root.style.setProperty("color-scheme", isSystemDark ? "dark" : "light");
    }
    saveItem("yode-theme-mode", themeMode);
  }, [themeMode]);

  useEffect(() => {
    const shells = document.querySelectorAll(".app-shell");
    shells.forEach((shell) => {
      if (translucentSidebar) {
        shell.classList.add("translucent-sidebar");
      } else {
        shell.classList.remove("translucent-sidebar");
      }
    });
    saveItem("yode-translucent-sidebar", translucentSidebar);
  }, [translucentSidebar]);

  useEffect(() => {
    if (usePointerCursors) {
      document.body.classList.add("use-pointers");
    } else {
      document.body.classList.remove("use-pointers");
    }
    saveItem("yode-use-pointers", usePointerCursors);
  }, [usePointerCursors]);

  useEffect(() => {
    const checkAndApplyMotion = () => {
      document.body.classList.remove("reduce-motion");
      if (reduceMotion === "on") {
        document.body.classList.add("reduce-motion");
      } else if (reduceMotion === "system") {
        const prefersReduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
        if (prefersReduced) {
          document.body.classList.add("reduce-motion");
        }
      }
    };
    checkAndApplyMotion();
    saveItem("yode-reduce-motion", reduceMotion);
  }, [reduceMotion]);

  useEffect(() => {
    document.body.classList.remove("font-smoothing", "no-font-smoothing");
    if (fontSmoothing) {
      document.body.classList.add("font-smoothing");
    } else {
      document.body.classList.add("no-font-smoothing");
    }
    saveItem("yode-font-smoothing", fontSmoothing);
  }, [fontSmoothing]);

  useEffect(() => {
    saveItem("yode-pet", pet);
    window.dispatchEvent(new CustomEvent("yode-pet-change", { detail: pet }));
  }, [pet]);

  useEffect(() => {
    saveItem("yode-diff-markers", diffMarkers);
  }, [diffMarkers]);

  const handleCopyTheme = () => {
    const themeJson = JSON.stringify(
      {
        themeMode,
        themeName,
        accentColor,
        backgroundColor,
        foregroundColor,
        uiFont,
        codeFont,
        translucentSidebar,
        contrast,
        uiFontSize,
        codeFontSize,
        appScale,
        chatFontSize,
        sidebarFontSize,
        settingsFontSize,
        terminalFontSize,
        inspectorFontSize
      },
      null,
      2
    );
    navigator.clipboard.writeText(themeJson).then(() => {
      setStatusText("主题配置已成功复制到剪贴板。");
    }).catch(() => {
      setStatusText("复制主题配置失败。");
    });
  };

  const handleResetTheme = () => {
    setThemeMode("dark");
    setThemeName("Dracula");
    setAccentColor("#FF79C6");
    setBackgroundColor("#282A36");
    setForegroundColor("#F8F8F2");
    setUiFont("-apple-system, BlinkMacSystemFont, \"Segoe UI\", system-ui, sans-serif");
    setCodeFont("ui-monospace, \"SF Mono\", SFMono-Regular, Menlo, Monaco, Consolas, monospace");
    setTranslucentSidebar(true);
    setContrast(48);
    setAppScale(100);
    setUiFontSize(13);
    setChatFontSize(13.25);
    setSidebarFontSize(13);
    setSettingsFontSize(13);
    setCodeFontSize(12);
    setTerminalFontSize(12);
    setInspectorFontSize(12);
    setUsePointerCursors(false);
    setReduceMotion("system");
    setDiffMarkers("color");
    setFontSmoothing(true);
    setPet("Yode");
  };

  const [currentLang, setCurrentLang] = useState(() => localStorage.getItem("yode-language") || "zh");
  const isZh = currentLang === "zh";

  const t = (zhText: string, enText: string) => {
    return isZh ? zhText : enText;
  };

  useEffect(() => {
    const handleLangChange = (e: Event) => {
      const newLang = (e as CustomEvent).detail;
      setCurrentLang(newLang);
    };
    window.addEventListener("yode-language-change", handleLangChange);
    return () => window.removeEventListener("yode-language-change", handleLangChange);
  }, []);

  const renderFontSizeControl = (
    label: string,
    desc: string,
    value: number,
    min: number,
    max: number,
    step: number,
    unit: string,
    onChange: (value: number) => void
  ) => (
    <div className="form-row font-size-row">
      <div className="row-info">
        <span className="row-label">{label}</span>
        <span className="row-desc">{desc}</span>
      </div>
      <div className="font-size-control">
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => onChange(clampNumber(Number(e.target.value), min, max))}
          className="range-input font-size-range"
        />
        <div className="number-input-wrapper">
          <input
            type="number"
            min={min}
            max={max}
            step={step}
            value={value}
            onChange={(e) => onChange(clampNumber(Number(e.target.value), min, max))}
            className="number-input"
          />
          <span className="unit-label">{unit}</span>
        </div>
      </div>
    </div>
  );

  return (
    <div className="appearance-container">
      <div className="theme-preview-box">
        <div className="theme-preview-header">
          <span className="preview-label">{t("主题预览代码配置", "Theme preview code config")}</span>
        </div>
        <div className={`theme-preview-code ${diffMarkers === "symbols" ? "diff-symbols" : ""}`}>
          <div className="code-column code-removed">
            <div className="code-line">
              <span className="line-num">1</span>
              <span className="keyword">const</span> themePreview: <span className="type">ThemeConfig</span> = &#123;
            </div>
            <div className="code-line removed-line">
              <span className="line-num">2</span> surface: <span className="string">"sidebar"</span>,
            </div>
            <div className="code-line removed-line">
              <span className="line-num">3</span> accent: <span className="string">"{accentColor}"</span>,
            </div>
            <div className="code-line removed-line">
              <span className="line-num">4</span> contrast: <span className="number">{contrast}</span>,
            </div>
            <div className="code-line">
              <span className="line-num">5</span>&#125;;
            </div>
          </div>
          <div className="code-column code-added">
            <div className="code-line">
              <span className="line-num">1</span>
              <span className="keyword">const</span> themePreview: <span className="type">ThemeConfig</span> = &#123;
            </div>
            <div className="code-line added-line">
              <span className="line-num">2</span> surface:{" "}
              <span className="string">"{translucentSidebar ? "sidebar-translucent" : "sidebar-elevated"}"</span>,
            </div>
            <div className="code-line added-line">
              <span className="line-num">3</span> accent: <span className="string">"{accentColor}"</span>,
            </div>
            <div className="code-line added-line">
              <span className="line-num">4</span> contrast: <span className="number">{contrast}</span>,
            </div>
            <div className="code-line">
              <span className="line-num">5</span>&#125;;
            </div>
          </div>
        </div>
      </div>

      <div className="theme-card">
        <div className="form-row theme-mode-row">
          <div className="row-info">
            <span className="row-label">{t("主题模式", "Theme")}</span>
            <span className="row-desc">{t("使用亮色、暗色或匹配您的系统", "Use light, dark, or match your system")}</span>
          </div>
          <div className="theme-mode-buttons">
            <button
              className={`mode-btn ${themeMode === "light" ? "active" : ""}`}
              onClick={() => setThemeMode("light")}
              type="button"
            >
              <Sun size={14} />
              <span>{t("亮色", "Light")}</span>
            </button>
            <button
              className={`mode-btn ${themeMode === "dark" ? "active" : ""}`}
              onClick={() => setThemeMode("dark")}
              type="button"
            >
              <Moon size={14} />
              <span>{t("暗色", "Dark")}</span>
            </button>
            <button
              className={`mode-btn ${themeMode === "system" ? "active" : ""}`}
              onClick={() => setThemeMode("system")}
              type="button"
            >
              <Monitor size={14} />
              <span>{t("系统", "System")}</span>
            </button>
          </div>
        </div>

        <div className="divider" />

        <div className="form-row flex-row">
          <div className="row-info">
            <span className="row-label">{t("当前主题", "Theme Preset")}</span>
          </div>
          <div className="theme-actions-preset">
            <button className="text-action-btn" onClick={handleResetTheme} type="button">
              <Download size={13} />
              <span>{t("重置主题", "Reset theme")}</span>
            </button>
            <button className="text-action-btn" onClick={handleCopyTheme} type="button">
              <Copy size={13} />
              <span>{t("复制配置", "Copy theme")}</span>
            </button>
            <CustomSelect
              value={themeName}
              onChange={setThemeName}
              options={[
                { value: "Dracula", label: "Dracula", avatarText: "Aa", avatarBg: "rgba(255, 121, 198, 0.2)", avatarFg: "#FF79C6" },
                { value: "One Dark", label: "One Dark", avatarText: "Aa", avatarBg: "rgba(97, 175, 239, 0.2)", avatarFg: "#61AFEF" },
                { value: "Nord", label: "Nord", avatarText: "Aa", avatarBg: "rgba(136, 192, 208, 0.2)", avatarFg: "#88C0D0" },
                { value: "Monokai", label: "Monokai", avatarText: "Aa", avatarBg: "rgba(249, 38, 114, 0.2)", avatarFg: "#F92672" },
                { value: "Catppuccin", label: "Catppuccin", avatarText: "Aa", avatarBg: "rgba(245, 194, 231, 0.2)", avatarFg: "#F5C2E7" },
                { value: "GitHub Dark", label: "GitHub Dark", avatarText: "Aa", avatarBg: "rgba(88, 166, 255, 0.2)", avatarFg: "#58A6FF" },
                { value: "Solarized", label: "Solarized", avatarText: "Aa", avatarBg: "rgba(38, 139, 210, 0.2)", avatarFg: "#268BD2" },
                { value: "Gruvbox", label: "Gruvbox", avatarText: "Aa", avatarBg: "rgba(254, 128, 25, 0.2)", avatarFg: "#FE8019" },
                { value: "Ayu", label: "Ayu", avatarText: "Aa", avatarBg: "rgba(242, 151, 24, 0.2)", avatarFg: "#F29718" },
                { value: "Tokyo Night", label: "Tokyo Night", avatarText: "Aa", avatarBg: "rgba(122, 162, 247, 0.2)", avatarFg: "#7AA2F7" },
                { value: "Everforest", label: "Everforest", avatarText: "Aa", avatarBg: "rgba(167, 192, 128, 0.2)", avatarFg: "#A7C080" },
                { value: "Linear", label: "Linear", avatarText: "Aa", avatarBg: "rgba(94, 106, 210, 0.2)", avatarFg: "#5E6AD2" }
              ]}
              style={{ minWidth: "160px" }}
            />
          </div>
        </div>

        <div className="form-row flex-row">
          <span className="row-label">{t("主题主色", "Accent color")}</span>
          <ColorPicker value={accentColor} onChange={setAccentColor} />
        </div>

        <div className="form-row flex-row">
          <span className="row-label">{t("背景色", "Background color")}</span>
          <ColorPicker value={backgroundColor} onChange={setBackgroundColor} />
        </div>

        <div className="form-row flex-row">
          <span className="row-label">{t("前景色", "Foreground color")}</span>
          <ColorPicker value={foregroundColor} onChange={setForegroundColor} />
        </div>

        <div className="form-row flex-row">
          <span className="row-label">{t("UI 界面字体", "UI font")}</span>
          <input
            type="text"
            className="text-input text-field-font"
            value={uiFont}
            onChange={(e) => setUiFont(e.target.value)}
          />
        </div>

        <div className="form-row flex-row">
          <span className="row-label">{t("代码编辑器字体", "Code font")}</span>
          <input
            type="text"
            className="text-input text-field-font"
            value={codeFont}
            onChange={(e) => setCodeFont(e.target.value)}
          />
        </div>
      </div>

      <div className="theme-card font-size-section">
        {renderFontSizeControl(
          t("整体大小", "Overall size"),
          t("同时放大或缩小主要界面、对话、代码和终端字号", "Scale the main UI, chat, code, and terminal text together"),
          appScale,
          85,
          130,
          1,
          "%",
          setAppScale
        )}

        <div className="divider" />

        {renderFontSizeControl(
          t("界面字号", "UI font size"),
          t("影响顶部栏、按钮、设置项等基础界面文字", "Controls top bars, buttons, settings rows, and base UI text"),
          uiFontSize,
          10,
          18,
          0.5,
          "px",
          setUiFontSize
        )}

        <div className="divider" />

        {renderFontSizeControl(
          t("对话字号", "Chat font size"),
          t("影响用户消息、AI 回复、Markdown 正文和输入框", "Controls user messages, assistant replies, Markdown text, and the composer"),
          chatFontSize,
          11,
          20,
          0.25,
          "px",
          setChatFontSize
        )}

        <div className="divider" />

        {renderFontSizeControl(
          t("侧边栏字号", "Sidebar font size"),
          t("影响项目、对话列表和侧边导航文字", "Controls project names, conversation lists, and sidebar navigation"),
          sidebarFontSize,
          10,
          18,
          0.5,
          "px",
          setSidebarFontSize
        )}

        <div className="divider" />

        {renderFontSizeControl(
          t("设置页字号", "Settings font size"),
          t("影响设置侧栏、设置标题、说明文字和表单控件", "Controls settings sidebar, headings, descriptions, and form controls"),
          settingsFontSize,
          10,
          18,
          0.5,
          "px",
          setSettingsFontSize
        )}

        <div className="divider" />

        {renderFontSizeControl(
          t("代码字号", "Code font size"),
          t("影响内联代码、代码块、Diff 和工具输出", "Controls inline code, code blocks, diffs, and tool output"),
          codeFontSize,
          10,
          20,
          0.5,
          "px",
          setCodeFontSize
        )}

        <div className="divider" />

        {renderFontSizeControl(
          t("终端字号", "Terminal font size"),
          t("影响真实 PTY 终端内容，并会自动重新适配列宽", "Controls the PTY terminal text and refits the terminal columns"),
          terminalFontSize,
          10,
          22,
          0.5,
          "px",
          setTerminalFontSize
        )}

        <div className="divider" />

        {renderFontSizeControl(
          t("运行详情字号", "Inspector font size"),
          t("影响右侧运行详情、状态和工具摘要文字", "Controls the right inspector, status, and tool summary text"),
          inspectorFontSize,
          10,
          18,
          0.5,
          "px",
          setInspectorFontSize
        )}
      </div>

      <div className="theme-card">
        <div className="form-row flex-row">
          <span className="row-label">{t("毛玻璃模糊侧边栏", "Translucent sidebar")}</span>
          <label className="switch-wrapper">
            <input
              type="checkbox"
              checked={translucentSidebar}
              onChange={(e) => setTranslucentSidebar(e.target.checked)}
            />
            <span className="switch-slider" />
          </label>
        </div>

        <div className="form-row flex-row">
          <span className="row-label">{t("全局对比度", "Contrast")}</span>
          <div className="slider-wrapper">
            <input
              type="range"
              min="0"
              max="100"
              value={contrast}
              onChange={(e) => setContrast(Number(e.target.value))}
              className="range-input"
            />
            <span className="slider-value">{contrast}</span>
          </div>
        </div>
      </div>

      <div className="theme-card advanced-section">
        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("使用手型指针", "Use pointer cursors")}</span>
            <span className="row-desc">
              {t("悬停在可交互元素上时，将光标更改为手型", "Change the cursor to a pointer when hovering over interactive elements")}
            </span>
          </div>
          <label className="switch-wrapper">
            <input
              type="checkbox"
              checked={usePointerCursors}
              onChange={(e) => setUsePointerCursors(e.target.checked)}
            />
            <span className="switch-slider" />
          </label>
        </div>

        <div className="divider" />

        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("减少动画效果", "Reduce motion")}</span>
            <span className="row-desc">{t("减少界面动效，或匹配您的系统偏好", "Reduce animations or match your system")}</span>
          </div>
          <div className="segmented-control">
            {(["system", "on", "off"] as const).map((opt) => (
              <button
                key={opt}
                onClick={() => setReduceMotion(opt)}
                className={`segmented-btn ${reduceMotion === opt ? "active" : ""}`}
                type="button"
              >
                {opt === "system" ? t("系统", "System") : opt === "on" ? t("开启", "On") : t("关闭", "Off")}
              </button>
            ))}
          </div>
        </div>

        <div className="divider" />

        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("Diff 标记风格", "Diff markers")}</span>
            <span className="row-desc">
              {t("使用背景色块，或者在每一行修改前显示 +/- 符号", "Use colored backgrounds or show + and - symbols on each changed line")}
            </span>
          </div>
          <div className="segmented-control">
            <button
              onClick={() => setDiffMarkers("color")}
              className={`segmented-btn ${diffMarkers === "color" ? "active" : ""}`}
              type="button"
            >
              {t("彩色背景", "Color")}
            </button>
            <button
              onClick={() => setDiffMarkers("symbols")}
              className={`segmented-btn ${diffMarkers === "symbols" ? "active" : ""}`}
              type="button"
            >
              {t("显示 +/-", "+/-")}
            </button>
          </div>
        </div>

        <div className="divider" />

        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("字体平滑 (抗锯齿)", "Font Smoothing")}</span>
            <span className="row-desc">{t("使用 macOS 原生字体抗锯齿优化效果", "Use native macOS font anti-aliasing")}</span>
          </div>
          <label className="switch-wrapper">
            <input
              type="checkbox"
              checked={fontSmoothing}
              onChange={(e) => setFontSmoothing(e.target.checked)}
            />
            <span className="switch-slider" />
          </label>
        </div>
      </div>

      <div className="theme-card pet-section">
        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("电子宠物", "Pets")}</span>
            <span className="row-desc">{t("选择后会显示在主侧边栏底部", "Shows in the main sidebar footer")}</span>
          </div>
          <CustomSelect
            value={pet}
            onChange={setPet}
            options={[
              { value: "Yode", label: t("Yode 宠物", "Yode selected"), avatarText: "🐱", avatarBg: "rgba(255,255,255,0.06)" },
              { value: "Cat", label: t("猫猫", "Cat selected"), avatarText: "🐈", avatarBg: "rgba(255,255,255,0.06)" },
              { value: "Dog", label: t("狗狗", "Dog selected"), avatarText: "🐕", avatarBg: "rgba(255,255,255,0.06)" },
              { value: "None", label: t("无", "None"), avatarText: "🚫", avatarBg: "rgba(255,255,255,0.06)" }
            ]}
            style={{ minWidth: "165px" }}
          />
        </div>
      </div>
      {statusText && (
        <div style={{ fontSize: "11px", color: "var(--text-soft)" }}>
          {statusText}
        </div>
      )}
    </div>
  );
}
