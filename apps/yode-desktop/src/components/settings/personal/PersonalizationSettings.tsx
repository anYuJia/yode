import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CustomSelect } from "../../CustomSelect";
import { isTauriRuntime, loadDesktopSetting, saveDesktopSetting } from "../../../lib/desktopSettings";

type PersonalizationState = {
  personality: string;
  customInstructions: string;
  enableMemories: boolean;
  skipToolChats: boolean;
};

export function PersonalizationSettings({ isZh, t }: { isZh: boolean; t: (zh: string, en: string) => string }) {
  const [personality, setPersonality] = useState(() => localStorage.getItem("yode-personality") || "Friendly");
  const [customInstructions, setCustomInstructions] = useState(() => localStorage.getItem("yode-custom-instructions") || "");
  const [enableMemories, setEnableMemories] = useState(() => localStorage.getItem("yode-enable-memories") === "true");
  const [skipToolChats, setSkipToolChats] = useState(() => localStorage.getItem("yode-skip-tool-chats") === "true");
  const [statusText, setStatusText] = useState("");

  const saveVal = (key: string, val: unknown) => {
    void saveDesktopSetting(key, val);
  };

  useEffect(() => {
    if (isTauriRuntime()) {
      void invoke<PersonalizationState>("personalization_state_get")
        .then((state) => {
          setPersonality(state.personality);
          setCustomInstructions(state.customInstructions);
          setEnableMemories(state.enableMemories);
          setSkipToolChats(state.skipToolChats);
        })
        .catch(() => {
          void loadDesktopSetting("yode-personality", personality).then(setPersonality);
          void loadDesktopSetting("yode-custom-instructions", customInstructions).then(setCustomInstructions);
          void loadDesktopSetting("yode-enable-memories", enableMemories).then(setEnableMemories);
          void loadDesktopSetting("yode-skip-tool-chats", skipToolChats).then(setSkipToolChats);
        });
      return;
    }
    void loadDesktopSetting("yode-personality", personality).then(setPersonality);
    void loadDesktopSetting("yode-custom-instructions", customInstructions).then(setCustomInstructions);
    void loadDesktopSetting("yode-enable-memories", enableMemories).then(setEnableMemories);
    void loadDesktopSetting("yode-skip-tool-chats", skipToolChats).then(setSkipToolChats);
  }, []);

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
            onClick={() => {
              saveVal("yode-custom-instructions", customInstructions);
              setStatusText(t("全局指令已保存到桌面设置。", "Global instructions saved to desktop settings."));
            }}
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
              onClick={async () => {
                try {
                  if (isTauriRuntime()) {
                    const result = await invoke<{ ok: boolean; message: string }>("personalization_reset_memories");
                    setStatusText(result.message);
                  } else {
                    setStatusText(t("长期记忆设置已重置。", "Memory settings reset."));
                  }
                  saveVal("yode-enable-memories", false);
                  saveVal("yode-skip-tool-chats", false);
                  setEnableMemories(false);
                  setSkipToolChats(false);
                } catch (err) {
                  setStatusText(
                    err instanceof Error
                      ? err.message
                      : t("重置长期记忆失败。", "Failed to reset memories.")
                  );
                }
              }}
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
      {statusText && (
        <div style={{ fontSize: "11px", color: "var(--text-soft)" }}>
          {statusText}
        </div>
      )}
    </div>
  );
}
