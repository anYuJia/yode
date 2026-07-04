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
import {
  applyAppearanceSettings,
  applyTranslucentSidebarSetting,
  clampNumber,
  DEFAULT_CODE_FONT,
  DEFAULT_UI_FONT,
  dispatchAppearanceChange,
  dispatchPetChange,
  DiffMarkerMode,
  LANGUAGE_CHANGE_EVENT,
  loadAppLanguage,
  loadAppearanceSettings,
  ReduceMotionMode,
  saveAppearanceSettings,
  ThemeMode,
  themePresetForMode
} from "../../../lib/appearanceSettings";

export function AppearanceSettings() {
  const initialAppearance = loadAppearanceSettings();
  const [themeMode, setThemeMode] = useState<ThemeMode>(initialAppearance.themeMode);
  const [themeName, setThemeName] = useState(initialAppearance.themeName);
  const [accentColor, setAccentColor] = useState(initialAppearance.accentColor);
  const [backgroundColor, setBackgroundColor] = useState(initialAppearance.backgroundColor);
  const [foregroundColor, setForegroundColor] = useState(initialAppearance.foregroundColor);
  const [uiFont, setUiFont] = useState(initialAppearance.uiFont);
  const [codeFont, setCodeFont] = useState(initialAppearance.codeFont);
  const [translucentSidebar, setTranslucentSidebar] = useState(initialAppearance.translucentSidebar);
  const [contrast, setContrast] = useState(initialAppearance.contrast);
  const [usePointerCursors, setUsePointerCursors] = useState(initialAppearance.usePointerCursors);
  const [reduceMotion, setReduceMotion] = useState<ReduceMotionMode>(initialAppearance.reduceMotion);
  const [uiFontSize, setUiFontSize] = useState(initialAppearance.uiFontSize);
  const [codeFontSize, setCodeFontSize] = useState(initialAppearance.codeFontSize);
  const [appScale, setAppScale] = useState(initialAppearance.appScale);
  const [chatFontSize, setChatFontSize] = useState(initialAppearance.chatFontSize);
  const [sidebarFontSize, setSidebarFontSize] = useState(initialAppearance.sidebarFontSize);
  const [settingsFontSize, setSettingsFontSize] = useState(initialAppearance.settingsFontSize);
  const [terminalFontSize, setTerminalFontSize] = useState(initialAppearance.terminalFontSize);
  const [inspectorFontSize, setInspectorFontSize] = useState(initialAppearance.inspectorFontSize);
  const [diffMarkers, setDiffMarkers] = useState<DiffMarkerMode>(initialAppearance.diffMarkers);
  const [fontSmoothing, setFontSmoothing] = useState(initialAppearance.fontSmoothing);
  const [pet, setPet] = useState(initialAppearance.pet);
  const [statusText, setStatusText] = useState("");

  useEffect(() => {
    const preset = themePresetForMode(themeName, themeMode);
    setAccentColor(preset.accent);
    setBackgroundColor(preset.bg);
    setForegroundColor(preset.fg);
  }, [themeName, themeMode]);

  useEffect(() => {
    const settings = {
      themeMode,
      themeName,
      accentColor,
      backgroundColor,
      foregroundColor,
      uiFont,
      codeFont,
      translucentSidebar,
      contrast,
      usePointerCursors,
      reduceMotion,
      uiFontSize,
      codeFontSize,
      appScale,
      chatFontSize,
      sidebarFontSize,
      settingsFontSize,
      terminalFontSize,
      inspectorFontSize,
      diffMarkers,
      fontSmoothing,
      pet
    };
    applyAppearanceSettings(settings);
    applyTranslucentSidebarSetting(translucentSidebar);
    saveAppearanceSettings(settings);
    dispatchAppearanceChange();
  }, [
    themeMode,
    themeName,
    accentColor,
    backgroundColor,
    foregroundColor,
    uiFont,
    codeFont,
    translucentSidebar,
    codeFontSize,
    contrast,
    usePointerCursors,
    reduceMotion,
    uiFontSize,
    appScale,
    chatFontSize,
    sidebarFontSize,
    settingsFontSize,
    terminalFontSize,
    inspectorFontSize,
    diffMarkers,
    fontSmoothing,
    pet
  ]);

  useEffect(() => {
    dispatchPetChange(pet);
  }, [pet]);

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
    setUiFont(DEFAULT_UI_FONT);
    setCodeFont(DEFAULT_CODE_FONT);
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

  const [currentLang, setCurrentLang] = useState(() => loadAppLanguage());
  const isZh = currentLang === "zh";

  const t = (zhText: string, enText: string) => {
    return isZh ? zhText : enText;
  };

  useEffect(() => {
    const handleLangChange = (e: Event) => {
      const newLang = (e as CustomEvent).detail;
      setCurrentLang(newLang);
    };
    window.addEventListener(LANGUAGE_CHANGE_EVENT, handleLangChange);
    return () => window.removeEventListener(LANGUAGE_CHANGE_EVENT, handleLangChange);
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
