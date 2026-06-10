import React, { useState, useEffect } from "react";
import {
  Sun,
  Moon,
  Monitor,
  Download,
  Copy,
  Search,
  SlidersHorizontal,
  X,
  Bot
} from "lucide-react";
import { CustomSelect } from "../CustomSelect";
import { ColorPicker } from "../ColorPicker";

// ----------------------------------------------------
// Appearance Settings Component
// ----------------------------------------------------
export function AppearanceSettings() {
  const [themeMode, setThemeMode] = useState<"light" | "dark" | "system">(
    () => (localStorage.getItem("yode-theme-mode") as any) || "dark"
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
    () => (localStorage.getItem("yode-reduce-motion") as any) || "system"
  );
  const [uiFontSize, setUiFontSize] = useState(() => {
    const val = localStorage.getItem("yode-ui-font-size");
    return val === null ? 13 : Number(val);
  });
  const [codeFontSize, setCodeFontSize] = useState(() => {
    const val = localStorage.getItem("yode-code-font-size");
    return val === null ? 12 : Number(val);
  });
  const [diffMarkers, setDiffMarkers] = useState<"color" | "symbols">(
    () => (localStorage.getItem("yode-diff-markers") as any) || "color"
  );
  const [fontSmoothing, setFontSmoothing] = useState(() => {
    const val = localStorage.getItem("yode-font-smoothing");
    return val === null ? true : val === "true";
  });
  const [pet, setPet] = useState(
    () => localStorage.getItem("yode-pet") || "Yode"
  );

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

  const saveItem = (key: string, val: any) => {
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
    root.style.setProperty("--code-font-size", `${codeFontSize}px`);
    root.style.setProperty("--contrast-val", String(contrast));
    root.style.fontSize = `${uiFontSize}px`;

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
  }, [accentColor, backgroundColor, foregroundColor, uiFont, codeFont, codeFontSize, contrast, uiFontSize]);

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
        codeFontSize
      },
      null,
      2
    );
    navigator.clipboard.writeText(themeJson).then(() => {
      alert("主题配置已成功复制到剪贴板！");
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
    setUiFontSize(13);
    setCodeFontSize(12);
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
              <span>{t("导入/重置", "Reset theme")}</span>
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
            <span className="row-label">{t("UI 界面字号", "UI font size")}</span>
            <span className="row-desc">{t("调整 Yode 整体界面的基本字号", "Adjust the base size used for the Yode UI")}</span>
          </div>
          <div className="number-input-wrapper">
            <input
              type="number"
              value={uiFontSize}
              onChange={(e) => setUiFontSize(Number(e.target.value))}
              className="number-input"
            />
            <span className="unit-label">px</span>
          </div>
        </div>

        <div className="divider" />

        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("代码字号", "Code font size")}</span>
            <span className="row-desc">{t("调整对话和对比视图中的代码字号", "Adjust the base size used for code across chats and diffs")}</span>
          </div>
          <div className="number-input-wrapper">
            <input
              type="number"
              value={codeFontSize}
              onChange={(e) => setCodeFontSize(Number(e.target.value))}
              className="number-input"
            />
            <span className="unit-label">px</span>
          </div>
        </div>

        <div className="divider" />

        <div className="form-row">
          <div className="row-info">
            <span className="row-label">{t("Diff 标记风格", "Diff markers")}</span>
            <span className="row-desc">
              {t("使用背景色块，或者在每一行修改前显示 +/- 符号", "Use colored bars and backgrounds or show + and - symbols on each changed line")}
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
            <span className="row-desc">{t("已选 Yode 宠物", "Yode selected")}</span>
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
    </div>
  );
}

// ----------------------------------------------------
// Configuration Settings Component
// ----------------------------------------------------
export function ConfigurationSettings({ isZh, t }: { isZh: boolean; t: (zh: string, en: string) => string }) {
  const [configScope, setConfigScope] = useState(() => localStorage.getItem("yode-config-scope") || "User config");
  const [approvalPolicy, setApprovalPolicy] = useState(() => localStorage.getItem("yode-config-approval") || "On request");
  const [sandboxSettings, setSandboxSettings] = useState(() => localStorage.getItem("yode-config-sandbox") || "Read only");
  const [exposeDeps, setExposeDeps] = useState(() => localStorage.getItem("yode-expose-deps") !== "false");

  const saveVal = (key: string, val: any) => localStorage.setItem(key, String(val));

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div style={{ fontSize: "12px", color: "var(--text-soft)" }}>
        {t("配置审批策略和沙箱设置", "Configure approval policy and sandbox settings")}{" "}
        <a href="#learn" style={{ color: "var(--accent)", textDecoration: "none" }}>
          {t("了解更多", "Learn more")}
        </a>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
        <span
          style={{
            fontSize: "11px",
            fontWeight: "700",
            color: "var(--text-soft)",
            textTransform: "uppercase",
            letterSpacing: "0.5px"
          }}
        >
          {t("自定义 config.toml 设置", "Custom config.toml settings")}
        </span>

        <div className="theme-card" style={{ padding: "16px", display: "flex", flexDirection: "column", gap: "14px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <CustomSelect
              value={configScope}
              onChange={(val) => {
                setConfigScope(val);
                saveVal("yode-config-scope", val);
              }}
              options={[
                { value: "User config", label: t("用户配置", "User config"), avatarText: "👤" },
                { value: "Project config", label: t("项目配置", "Project config"), avatarText: "📁" }
              ]}
              style={{ minWidth: "150px" }}
            />
            <a
              href="#open"
              style={{ fontSize: "11px", color: "var(--text-soft)", textDecoration: "none" }}
              className="hover-link"
            >
              {t("打开 config.toml ↗", "Open config.toml ↗")}
            </a>
          </div>

          <div style={{ height: "1px", background: "var(--line-soft)" }} />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("审批策略", "Approval policy")}</span>
              <span className="row-desc">{t("选择 Yode 何时需要确认请求", "Choose when Yode asks for approval")}</span>
            </div>
            <CustomSelect
              value={approvalPolicy}
              onChange={(val) => {
                setApprovalPolicy(val);
                saveVal("yode-config-approval", val);
              }}
              options={[
                { value: "On request", label: t("询问确认", "On request") },
                { value: "Always auto-approve", label: t("始终自动允许", "Always auto-approve") },
                { value: "Never approve", label: t("从不允许", "Never approve") }
              ]}
              style={{ minWidth: "160px" }}
            />
          </div>

          <div style={{ height: "1px", background: "var(--line-soft)" }} />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("沙箱设置", "Sandbox settings")}</span>
              <span className="row-desc">
                {t("选择 Yode 执行命令时的文件访问权限", "Choose how much Yode can do when running commands")}
              </span>
            </div>
            <CustomSelect
              value={sandboxSettings}
              onChange={(val) => {
                setSandboxSettings(val);
                saveVal("yode-config-sandbox", val);
              }}
              options={[
                { value: "Read only", label: t("只读", "Read only") },
                { value: "Full write access", label: t("读写访问", "Full write access") },
                { value: "Restricted", label: t("限制范围", "Restricted") }
              ]}
              style={{ minWidth: "160px" }}
            />
          </div>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
        <span
          style={{
            fontSize: "11px",
            fontWeight: "700",
            color: "var(--text-soft)",
            textTransform: "uppercase",
            letterSpacing: "0.5px"
          }}
        >
          {t("工作区依赖项", "Workspace Dependencies")}
        </span>
        <div className="theme-card">
          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("当前版本", "Current version")}</span>
            </div>
            <span style={{ fontSize: "12px", fontFamily: "var(--font-code)", color: "var(--text-muted)", alignSelf: "center" }}>
              26.601.10930
            </span>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("Yode 依赖项", "Yode dependencies")}</span>
              <span className="row-desc">
                {t("允许 Yode 安装并向工作区暴露 Node.js & Python 工具", "Allow Yode to install and expose bundled Node.js and Python tools")}
              </span>
            </div>
            <label className="switch-wrapper">
              <input
                type="checkbox"
                checked={exposeDeps}
                onChange={(e) => {
                  setExposeDeps(e.target.checked);
                  saveVal("yode-expose-deps", e.target.checked);
                }}
              />
              <span className="switch-slider" />
            </label>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("诊断 Yode 工作区问题", "Diagnose issues in Yode Workspace")}</span>
              <span className="row-desc">{t("检查当前环境包并记录诊断日志", "Checks the current bundle and records diagnostic logs")}</span>
            </div>
            <button
              className="secondary-button"
              style={{ display: "flex", alignItems: "center", gap: "6px", paddingInline: "12px", height: "28px" }}
              type="button"
            >
              <Search size={12} />
              <span>{t("诊断", "Diagnose")}</span>
            </button>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("重置并安装工作区", "Reset and install Workspace")}</span>
              <span className="row-desc">{t("删除本地缓存，重新下载并重新加载工具", "Deletes the local bundle, downloads it again, and reloads tools")}</span>
            </div>
            <button
              className="secondary-button"
              style={{
                color: "oklch(67% 0.15 28)",
                borderColor: "rgba(224, 80, 80, 0.2)",
                display: "flex",
                alignItems: "center",
                gap: "6px",
                paddingInline: "12px",
                height: "28px"
              }}
              type="button"
            >
              <Download size={12} />
              <span>{t("重新安装", "Reinstall")}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ----------------------------------------------------
// Personalization Settings Component
// ----------------------------------------------------
export function PersonalizationSettings({ isZh, t }: { isZh: boolean; t: (zh: string, en: string) => string }) {
  const [personality, setPersonality] = useState(() => localStorage.getItem("yode-personality") || "Friendly");
  const [customInstructions, setCustomInstructions] = useState(() => localStorage.getItem("yode-custom-instructions") || "");
  const [enableMemories, setEnableMemories] = useState(() => localStorage.getItem("yode-enable-memories") === "true");
  const [skipToolChats, setSkipToolChats] = useState(() => localStorage.getItem("yode-skip-tool-chats") === "true");

  const saveVal = (key: string, val: any) => localStorage.setItem(key, String(val));

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div className="theme-card" style={{ padding: "16px" }}>
        <div className="form-row" style={{ alignItems: "center" }}>
          <div className="row-info">
            <span className="row-label">{t("人设风格", "Personality")}</span>
            <span className="row-desc">{t("选择 Yode 对话时的默认语气风格", "Choose a default tone for Yode responses")}</span>
          </div>
          <CustomSelect
            value={personality}
            onChange={(val) => {
              setPersonality(val);
              saveVal("yode-personality", val);
            }}
            options={[
              { value: "Friendly", label: t("友好热情", "Friendly") },
              { value: "Professional", label: t("专业严谨", "Professional") },
              { value: "Concise", label: t("简洁干练", "Concise") }
            ]}
            style={{ minWidth: "160px" }}
          />
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
        <span
          style={{
            fontSize: "11px",
            fontWeight: "700",
            color: "var(--text-soft)",
            textTransform: "uppercase",
            letterSpacing: "0.5px"
          }}
        >
          {t("自定义指令", "Custom instructions")}
        </span>
        <span style={{ fontSize: "11px", color: "var(--text-soft)", marginBottom: "4px" }}>
          {t("为这台主机上的所有任务向 Yode 提供额外指令和上下文。", "Give Yode extra instructions and context for all tasks on this host.")}{" "}
          <a href="#learn" style={{ color: "var(--accent)", textDecoration: "none" }}>
            {t("了解更多", "Learn more")}
          </a>
        </span>
        <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
          <textarea
            placeholder={t("添加您的自定义全局指令...", "Add your custom instructions...")}
            value={customInstructions}
            onChange={(e) => {
              setCustomInstructions(e.target.value);
              saveVal("yode-custom-instructions", e.target.value);
            }}
            style={{
              width: "100%",
              height: "160px",
              background: "var(--field)",
              border: "1px solid var(--line-soft)",
              borderRadius: "var(--radius)",
              padding: "12px",
              fontSize: "12px",
              color: "var(--text)",
              fontFamily: "var(--font-ui)",
              resize: "none",
              outline: "none"
            }}
          />
          <button
            onClick={() => alert(t("全局指令已成功保存！", "Global instructions saved successfully!"))}
            className="secondary-button"
            type="button"
            style={{ alignSelf: "flex-end", height: "28px", paddingInline: "20px", background: "var(--panel-raised)" }}
          >
            {t("保存", "Save")}
          </button>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
        <span
          style={{
            fontSize: "11px",
            fontWeight: "700",
            color: "var(--text-soft)",
            textTransform: "uppercase",
            letterSpacing: "0.5px"
          }}
        >
          {t("长期记忆（实验性）", "Memory (experimental)")}
        </span>
        <span style={{ fontSize: "11px", color: "var(--text-soft)", marginBottom: "4px" }}>
          {t("配置 Yode 如何收集、保留和整合对话记忆。", "Configure how Yode collects, retains, and consolidates memories.")}{" "}
          <a href="#learn" style={{ color: "var(--accent)", textDecoration: "none" }}>
            {t("了解更多", "Learn more")}
          </a>
        </span>
        <div className="theme-card">
          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("启用长期记忆", "Enable memories")}</span>
              <span className="row-desc">
                {t("从历史会话中生成长效记忆并在新对话中携带", "Generate new memories from chats and bring them into new chats")}
              </span>
            </div>
            <label className="switch-wrapper">
              <input
                type="checkbox"
                checked={enableMemories}
                onChange={(e) => {
                  setEnableMemories(e.target.checked);
                  saveVal("yode-enable-memories", e.target.checked);
                }}
              />
              <span className="switch-slider" />
            </label>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("跳过包含工具的对话", "Skip tool-assisted chats")}</span>
              <span className="row-desc">
                {t("对使用了 MCP 工具或进行网页搜索的对话不生成长期记忆", "Do not generate memories from chats that used MCP tools or web search")}
              </span>
            </div>
            <label className="switch-wrapper">
              <input
                type="checkbox"
                checked={skipToolChats}
                onChange={(e) => {
                  setSkipToolChats(e.target.checked);
                  saveVal("yode-skip-tool-chats", e.target.checked);
                }}
              />
              <span className="switch-slider" />
            </label>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("重置记忆内容", "Reset memories")}</span>
              <span className="row-desc">{t("彻底清空当前 Yode 保存的所有长期记忆", "Delete all Yode memories")}</span>
            </div>
            <button
              onClick={() => alert(t("长期记忆已被重置清空。", "All long-term memories have been reset."))}
              className="secondary-button"
              style={{
                color: "oklch(67% 0.15 28)",
                borderColor: "rgba(224, 80, 80, 0.2)",
                paddingInline: "14px",
                height: "28px"
              }}
              type="button"
            >
              {t("重置", "Reset")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ----------------------------------------------------
// Keyboard Shortcuts Settings Component
// ----------------------------------------------------
export function KeyboardShortcutsSettings({ isZh, t }: { isZh: boolean; t: (zh: string, en: string) => string }) {
  const [searchQuery, setSearchQuery] = useState("");

  const [bindings, setBindings] = useState<
    Array<{
      id: string;
      cmdZh: string;
      cmdEn: string;
      descZh: string;
      descEn: string;
      keys: string[];
    }>
  >([
    { id: "archive", cmdZh: "归档对话", cmdEn: "Archive chat", descZh: "归档当前活动的对话", descEn: "Archive the current chat", keys: ["⇧⌘A"] },
    { id: "newchat", cmdZh: "新建对话", cmdEn: "New chat", descZh: "发起一个新的对话", descEn: "Start a new chat", keys: ["⌘N", "⇧⌘O"] },
    { id: "sidechat", cmdZh: "打开侧边栏对话", cmdEn: "Open side chat", descZh: "在侧边栏中打开当前对话", descEn: "Open the current chat in a side chat", keys: [] },
    { id: "newwin", cmdZh: "在新窗口打开", cmdEn: "Open in new window", descZh: "在新窗口中打开当前对话", descEn: "Open the current chat in a new window", keys: [] },
    { id: "quickchat", cmdZh: "新建快速对话", cmdEn: "New quick chat", descZh: "在快速输入框中启动轻量对话", descEn: "Start a lightweight chat in the quick composer", keys: ["⌥⌘N"] },
    { id: "pin", cmdZh: "固定/取消固定", cmdEn: "Toggle pin", descZh: "固定或取消固定当前对话", descEn: "Pin or unpin the current chat", keys: ["⌥⌘P"] },
    { id: "find", cmdZh: "查找", cmdEn: "Find", descZh: "在当前对话中搜索内容", descEn: "Search the current chat", keys: ["⌘F"] },
    { id: "addressbar", cmdZh: "聚焦浏览器地址栏", cmdEn: "Focus browser address bar", descZh: "将焦点定位到应用内浏览器地址栏", descEn: "Focus the in-app browser address bar", keys: ["⌘L"] },
    { id: "back", cmdZh: "后退", cmdEn: "Back", descZh: "在导航历史记录中向后退一步", descEn: "Go back in navigation history", keys: ["⌘[", "Mouse Back"] },
    { id: "forward", cmdZh: "前进", cmdEn: "Forward", descZh: "在导航历史记录中向前进一步", descEn: "Go forward in navigation history", keys: ["⌘]", "Mouse Forward"] },
    { id: "next_chat_tab", cmdZh: "下一个对话或标签页", cmdEn: "Next chat or tab", descZh: "切换至下一个对话或标签页", descEn: "Switch to the next chat or tab", keys: ["⇧⌘]", "⌥⌘Right"] },
    { id: "prev_recent", cmdZh: "上一个最近查看的对话或标签页", cmdEn: "Previous recently viewed chat or tab", descZh: "轮转切换至上一个或最近查看的对话或标签页", descEn: "Cycle to the previous recently viewed chat or tab", keys: ["⌃⇧Tab"] },
    { id: "prev_chat_tab", cmdZh: "上一个对话或标签页", cmdEn: "Previous chat or tab", descZh: "切换至上一个对话或标签页", descEn: "Switch to the previous chat or tab", keys: ["⇧⌘[", "⌥⌘Left"] },
    { id: "open_browser_tab", cmdZh: "打开浏览器标签页", cmdEn: "Open browser tab", descZh: "打开一个新的浏览器标签页", descEn: "Open a browser tab", keys: ["⌘T"] },
    { id: "open_review_tab", cmdZh: "打开代码审查标签页", cmdEn: "Open review tab", descZh: "打开代码审查标签页", descEn: "Open the review tab", keys: ["⌃⇧G"] },
    { id: "toggle_bottom_panel", cmdZh: "显示/隐藏底部面板", cmdEn: "Toggle bottom panel", descZh: "显示或隐藏底部面板", descEn: "Show or hide the bottom panel", keys: ["⌘J"] },
    { id: "toggle_browser_panel", cmdZh: "显示/隐藏浏览器面板", cmdEn: "Toggle browser panel", descZh: "显示或隐藏浏览器面板", descEn: "Show or hide the browser panel", keys: ["⇧⌘B"] },
    { id: "toggle_sidebar", cmdZh: "显示/隐藏侧边栏", cmdEn: "Toggle sidebar", descZh: "显示或隐藏侧边栏", descEn: "Show or hide the sidebar", keys: ["⌘B"] },
    { id: "toggle_side_panel", cmdZh: "显示/隐藏侧栏面板", cmdEn: "Toggle side panel", descZh: "显示或隐藏侧栏面板", descEn: "Show or hide the side panel", keys: ["⌥⌘B"] },
    { id: "open_terminal", cmdZh: "打开终端", cmdEn: "Open terminal", descZh: "打开终端面板", descEn: "Open the terminal panel", keys: ["⌃`"] },
    { id: "env_action_1", cmdZh: "环境操作 1", cmdEn: "Environment action 1", descZh: "在此快捷键槽位中运行环境操作", descEn: "Run the environment action in this shortcut slot", keys: ["⇧⌘D"] },
    { id: "env_action_2", cmdZh: "环境操作 2", cmdEn: "Environment action 2", descZh: "在此快捷键槽位中运行环境操作", descEn: "Run the environment action in this shortcut slot", keys: [] },
    { id: "env_action_3", cmdZh: "环境操作 3", cmdEn: "Environment action 3", descZh: "在此快捷键槽位中运行环境操作", descEn: "Run the environment action in this shortcut slot", keys: [] },
    { id: "env_action_4", cmdZh: "环境操作 4", cmdEn: "Environment action 4", descZh: "在此快捷键槽位中运行环境操作", descEn: "Run the environment action in this shortcut slot", keys: [] },
    { id: "env_action_5", cmdZh: "环境操作 5", cmdEn: "Environment action 5", descZh: "在此快捷键槽位中运行环境操作", descEn: "Run the environment action in this shortcut slot", keys: [] },
    { id: "open_commit_push", cmdZh: "打开提交或推送选项", cmdEn: "Open commit or push options", descZh: "打开提交或推送选项", descEn: "Open commit or push options", keys: [] },
    { id: "create_pr", cmdZh: "创建拉取请求 (PR)", cmdEn: "Create PR", descZh: "打开拉取请求创建选项", descEn: "Open pull request creation options", keys: [] },
    { id: "open_folder", cmdZh: "打开文件夹", cmdEn: "Open folder", descZh: "向 Yode 添加本地项目", descEn: "Add a local project to Yode", keys: ["⌘O"] },
    { id: "force_reload_skills", cmdZh: "强制重新加载技能", cmdEn: "Force reload skills", descZh: "为当前上下文刷新技能目录", descEn: "Refresh the skill catalog for the current context", keys: [] },
    { id: "go_to_skills", cmdZh: "转到技能", cmdEn: "Go to skills", descZh: "浏览已安装和推荐的技能", descEn: "Browse installed and recommended skills", keys: [] },
    { id: "install_workspace", cmdZh: "安装 Yode 工作区", cmdEn: "Install Yode Workspace", descZh: "安装高级本地功能的依赖项", descEn: "Install dependencies for advanced local features", keys: [] },
    { id: "kbd_shortcuts", cmdZh: "键盘快捷键", cmdEn: "Keyboard shortcuts", descZh: "自定义键盘快捷键", descEn: "Customize keyboard shortcuts", keys: [] },
    { id: "mcp_config", cmdZh: "MCP", cmdEn: "MCP", descZh: "配置 MCP 服务器", descEn: "Configure MCP servers", keys: [] },
    { id: "personality_config", cmdZh: "人设风格", cmdEn: "Personality", descZh: "调整语气与响应风格", descEn: "Adjust tone and response style", keys: [] },
    { id: "feedback", cmdZh: "反馈", cmdEn: "Feedback", descZh: "向 Yode 团队发送产品反馈", descEn: "Send product feedback to the Yode team", keys: [] },
    { id: "logout", cmdZh: "退出登录", cmdEn: "Log out", descZh: "登出 Yode", descEn: "Sign out of Yode", keys: [] },
    { id: "manage_automations", cmdZh: "管理自动化", cmdEn: "Manage automations", descZh: "从当前上下文创建或管理自动化", descEn: "Create or manage automations from the current context", keys: [] },
    { id: "wake_pet", cmdZh: "唤醒宠物", cmdEn: "Wake Pet", descZh: "打开宠物悬停窗口", descEn: "Open the pet overlay", keys: [] },
    { id: "open_control_window", cmdZh: "打开控制窗口", cmdEn: "Open control window", descZh: "打开语音控制窗口", descEn: "Open the voice control window", keys: [] },
    { id: "settings", cmdZh: "设置", cmdEn: "Settings", descZh: "打开 Yode 设置", descEn: "Open Yode settings", keys: ["⌘,"] },
    { id: "approve_req", cmdZh: "批准请求", cmdEn: "Approve request", descZh: "批准当前请求", descEn: "Approve the active request", keys: ["↩"] },
    { id: "decline_req", cmdZh: "拒绝请求", cmdEn: "Decline request", descZh: "拒绝当前请求", descEn: "Decline the active request", keys: ["Escape"] },
    { id: "close_tab", cmdZh: "关闭", cmdEn: "Close", descZh: "关闭当前标签页或窗口", descEn: "Close the active tab or window", keys: ["⌘W"] },
    { id: "cycle_reasoning", cmdZh: "循环切换推理强度", cmdEn: "Cycle reasoning effort", descZh: "在输入框中循环切换推理强度", descEn: "Cycle through composer reasoning effort levels", keys: [] },
    { id: "decrease_reasoning", cmdZh: "降低推理强度", cmdEn: "Decrease reasoning effort", descZh: "降低当前输入框推理强度", descEn: "Decrease the current composer reasoning effort level", keys: [] },
    { id: "increase_reasoning", cmdZh: "提高推理强度", cmdEn: "Increase reasoning effort", descZh: "提高当前输入框推理强度", descEn: "Increase the current composer reasoning effort level", keys: [] },
    { id: "open_model_picker", cmdZh: "打开模型选择器", cmdEn: "Open model picker", descZh: "打开输入框模型选择器", descEn: "Open the composer model picker", keys: ["⌃⇧M"] },
    { id: "start_dictation", cmdZh: "启动听写", cmdEn: "Start dictation", descZh: "在当前输入框中启动听写", descEn: "Start dictation in the current composer", keys: ["⌃⇧D"] },
    { id: "toggle_voice", cmdZh: "切换语音模式", cmdEn: "Toggle voice mode", descZh: "启动或停止语音模式", descEn: "Start or stop voice mode", keys: ["⌃⇧V"] },
    { id: "send_msg", cmdZh: "发送消息", cmdEn: "Send message", descZh: "发送当前输入框中的消息", descEn: "Send the current composer message", keys: [] },
    { id: "toggle_fast", cmdZh: "切换快速模式", cmdEn: "Toggle Fast mode", descZh: "在当前输入框中开启或关闭快速模式", descEn: "Turn Fast mode on or off in the current composer", keys: [] },
    { id: "toggle_plan", cmdZh: "切换计划模式", cmdEn: "Toggle plan mode", descZh: "在当前输入框中开启或关闭计划模式", descEn: "Turn plan mode on or off in the current composer", keys: [] },
    { id: "copy_markdown", cmdZh: "复制为 Markdown", cmdEn: "Copy as Markdown", descZh: "将当前对话复制为 Markdown", descEn: "Copy the current chat as Markdown", keys: [] },
    { id: "copy_conv_path", cmdZh: "复制对话路径", cmdEn: "Copy conversation path", descZh: "复制当前对话路径", descEn: "Copy the current chat path", keys: ["⌥⇧⌘C"] },
    { id: "copy_deeplink", cmdZh: "复制深层链接", cmdEn: "Copy deeplink", descZh: "复制当前对话的深层链接", descEn: "Copy a deeplink to the current chat", keys: ["⌥⌘L"] },
    { id: "copy_session_id", cmdZh: "复制会话 ID", cmdEn: "Copy session id", descZh: "复制当前对话会话 ID", descEn: "Copy the current chat session ID", keys: ["⌥⌘C"] },
    { id: "copy_work_dir", cmdZh: "复制工作目录", cmdEn: "Copy working directory", descZh: "复制当前对话的工作目录", descEn: "Copy the current chat working directory", keys: ["⇧⌘C"] },
    { id: "fork_chat", cmdZh: "复刻对话", cmdEn: "Fork chat", descZh: "复刻当前对话", descEn: "Fork the current chat", keys: [] },
    { id: "rename_chat", cmdZh: "重命名对话", cmdEn: "Rename chat", descZh: "重命名当前对话", descEn: "Rename the current chat", keys: ["⌥⌘R"] },
    { id: "search_chats", cmdZh: "搜索对话", cmdEn: "Search Chats...", descZh: "搜索对话记录", descEn: "Search chats", keys: ["⌘G"] },
    { id: "search_files", cmdZh: "搜索文件", cmdEn: "Search Files...", descZh: "搜索工作区中的文件", descEn: "Search files", keys: ["⌘P"] },
    { id: "show_kbd_shortcuts", cmdZh: "显示键盘快捷键", cmdEn: "Show keyboard shortcuts", descZh: "立即显示可用快捷键", descEn: "Show the shortcuts available right now", keys: ["⌘?"] },
    { id: "go_to_chat_1", cmdZh: "转到对话 1", cmdEn: "Go to chat 1", descZh: "在此快捷键槽位中打开可见的对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘1"] },
    { id: "go_to_chat_2", cmdZh: "转到对话 2", cmdEn: "Go to chat 2", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘2"] },
    { id: "go_to_chat_3", cmdZh: "转到对话 3", cmdEn: "Go to chat 3", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘3"] },
    { id: "go_to_chat_4", cmdZh: "转到对话 4", cmdEn: "Go to chat 4", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘4"] },
    { id: "go_to_chat_5", cmdZh: "转到对话 5", cmdEn: "Go to chat 5", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘5"] },
    { id: "go_to_chat_6", cmdZh: "转到对话 6", cmdEn: "Go to chat 6", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘6"] },
    { id: "go_to_chat_7", cmdZh: "转到对话 7", cmdEn: "Go to chat 7", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘7"] },
    { id: "go_to_chat_8", cmdZh: "转到对话 8", cmdEn: "Go to chat 8", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘8"] },
    { id: "go_to_chat_9", cmdZh: "转到对话 9", cmdEn: "Go to chat 9", descZh: "在此快捷键槽位中打开可见 of 对话", descEn: "Open the visible chat in this shortcut slot", keys: ["⌘9"] },
    { id: "toggle_file_tree", cmdZh: "切换文件树", cmdEn: "Toggle File Tree", descZh: "切换文件树面板的显示与隐藏", descEn: "Toggle the file tree panel", keys: ["⇧⌘E"] },
    { id: "toggle_max_side_panel", cmdZh: "最大化/还原侧栏面板", cmdEn: "Toggle maximize side panel", descZh: "展开或还原侧栏面板", descEn: "Expand or restore the side panel", keys: [] },
    { id: "start_trace_rec", cmdZh: "开始/停止追踪录制", cmdEn: "Start Trace Recording", descZh: "启动或停止追踪录制", descEn: "Start or stop trace recording", keys: ["⇧⌘S"] }
  ]);

  const handleDeleteBinding = (id: string, keyIdx: number) => {
    setBindings((prev) =>
      prev.map((b) => {
        if (b.id === id) {
          const nextKeys = [...b.keys];
          nextKeys.splice(keyIdx, 1);
          return { ...b, keys: nextKeys };
        }
        return b;
      })
    );
  };

  const filteredBindings = bindings.filter(
    (b) =>
      b.cmdZh.toLowerCase().includes(searchQuery.toLowerCase()) ||
      b.cmdEn.toLowerCase().includes(searchQuery.toLowerCase()) ||
      b.descZh.toLowerCase().includes(searchQuery.toLowerCase()) ||
      b.descEn.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
      <div style={{ position: "relative", width: "100%" }}>
        <Search size={13} style={{ position: "absolute", left: "10px", top: "8px", color: "var(--text-soft)", opacity: 0.8 }} />
        <input
          type="text"
          placeholder={t("搜索快捷键...", "Search shortcuts...")}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{
            width: "100%",
            height: "28px",
            background: "var(--field)",
            border: "1px solid var(--line-soft)",
            borderRadius: "var(--radius)",
            paddingLeft: "28px",
            paddingRight: "28px",
            fontSize: "12px",
            color: "var(--text)",
            outline: "none"
          }}
        />
        <SlidersHorizontal size={13} style={{ position: "absolute", right: "10px", top: "8px", color: "var(--text-soft)", opacity: 0.8 }} />
      </div>

      <div className="theme-card" style={{ padding: "0 12px 12px" }}>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 200px",
            paddingBlock: "10px",
            borderBottom: "1px solid var(--line-soft)",
            fontSize: "11px",
            fontWeight: "700",
            color: "var(--text-soft)",
            textTransform: "uppercase",
            letterSpacing: "0.5px"
          }}
        >
          <span>{t("命令", "Command")}</span>
          <span>{t("快捷键", "Keybinding")}</span>
        </div>

        <div style={{ display: "flex", flexDirection: "column" }}>
          {filteredBindings.map((item) => (
            <div
              key={item.id}
              style={{
                display: "grid",
                gridTemplateColumns: "1fr 200px",
                paddingBlock: "12px",
                borderBottom: "1px solid var(--line-soft)",
                fontSize: "12px"
              }}
            >
              <div style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
                <span style={{ fontWeight: "600", color: "var(--text)" }}>{t(item.cmdZh, item.cmdEn)}</span>
                <span style={{ fontSize: "11px", color: "var(--text-soft)" }}>{t(item.descZh, item.descEn)}</span>
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "6px", justifyContent: "center" }}>
                {item.keys.length === 0 ? (
                  <span style={{ fontSize: "11px", color: "var(--text-soft)", opacity: 0.6 }}>Unassigned</span>
                ) : (
                  item.keys.map((k, idx) => (
                    <div
                      key={k}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "space-between",
                        background: "var(--field)",
                        border: "1px solid var(--line-soft)",
                        borderRadius: "var(--radius)",
                        paddingInline: "8px",
                        paddingBlock: "2px",
                        fontSize: "11px",
                        color: "var(--text)",
                        fontFamily: "var(--font-code)",
                        width: "100%",
                        maxWidth: "160px"
                      }}
                    >
                      <span>{k}</span>
                      <button
                        onClick={() => handleDeleteBinding(item.id, idx)}
                        type="button"
                        style={{
                          background: "transparent",
                          border: "none",
                          cursor: "pointer",
                          color: "var(--text-soft)",
                          padding: "1px 2px",
                          display: "flex",
                          alignItems: "center"
                        }}
                        onMouseEnter={(e) => (e.currentTarget.style.color = "oklch(67% 0.15 28)")}
                        onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-soft)")}
                      >
                        <X size={12} />
                      </button>
                    </div>
                  ))
                )}
              </div>
            </div>
          ))}
          {filteredBindings.length === 0 && (
            <div style={{ paddingBlock: "24px", textAlign: "center", color: "var(--text-soft)", fontSize: "12px" }}>
              {t("未找到匹配的快捷键命令", "No matching shortcut commands found")}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
