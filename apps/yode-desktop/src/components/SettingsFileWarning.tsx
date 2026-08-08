import React from "react";
import { AlertCircle } from "lucide-react";

export type SettingsFileWarningStatus = {
  loaded: boolean;
  path: string;
  error?: string | null;
};

export type SettingsFileWarningProps = {
  status: SettingsFileWarningStatus;
  isZh: boolean;
  onOpenFile: () => void;
  onRetry: () => void;
  onRestore: () => void;
};

const BUTTON_STYLE: React.CSSProperties = {
  border: "none",
  borderRadius: "var(--radius)",
  fontWeight: "600",
  fontSize: "11.5px",
  color: "var(--text)",
  background: "var(--field)",
  paddingInline: "10px",
  paddingBlock: "5px",
  cursor: "pointer",
  whiteSpace: "nowrap"
};

const DANGER_BUTTON_STYLE: React.CSSProperties = {
  border: "1px solid color-mix(in oklch, var(--danger, #e5484d), transparent 55%)",
  borderRadius: "var(--radius)",
  fontWeight: "600",
  fontSize: "11.5px",
  color: "var(--danger, #e5484d)",
  background: "transparent",
  paddingInline: "10px",
  paddingBlock: "4px",
  cursor: "pointer",
  whiteSpace: "nowrap"
};

/**
 * 桌面设置文件损坏/不可读时的提示条：明确告知“设置文件未加载”，
 * 并提供打开配置位置、重试读取与显式恢复三个可访问操作。
 * 状态为 loaded 时不渲染任何内容。
 */
export function SettingsFileWarning({
  status,
  isZh,
  onOpenFile,
  onRetry,
  onRestore
}: SettingsFileWarningProps) {
  const t = (zhText: string, enText: string) => (isZh ? zhText : enText);
  if (status.loaded) return null;

  return (
    <div
      className="settings-file-warning"
      role="alert"
      style={{
        width: "100%",
        maxWidth: "720px",
        display: "flex",
        gap: "12px",
        alignItems: "flex-start",
        padding: "12px 14px",
        marginTop: "12px",
        borderRadius: "var(--radius)",
        border: "1px solid color-mix(in oklch, var(--danger, #e5484d), transparent 45%)",
        background: "color-mix(in oklch, var(--danger, #e5484d), transparent 92%)"
      }}
    >
      <AlertCircle size={16} style={{ flexShrink: 0, marginTop: "2px", color: "var(--danger, #e5484d)" }} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <strong style={{ fontSize: "13px", color: "var(--text)" }}>
          {t("设置文件未加载", "Settings file not loaded")}
        </strong>
        <p style={{ margin: "4px 0 0", fontSize: "12px", lineHeight: 1.5, color: "var(--text-muted)" }}>
          {t(
            "无法读取或解析桌面设置文件，当前页面显示的是本地默认值，修改可能无法保存。",
            "The desktop settings file cannot be read or parsed. The page is showing local defaults and changes may not be saved."
          )}
        </p>
        {status.error ? (
          <p style={{ margin: "4px 0 0", fontSize: "11px", lineHeight: 1.4, color: "var(--text-soft)" }}>
            {status.error}
          </p>
        ) : null}
      </div>
      <div style={{ display: "flex", gap: "6px", flexShrink: 0, alignItems: "center" }}>
        <button type="button" onClick={onOpenFile} style={BUTTON_STYLE}>
          {t("打开配置位置", "Open settings file")}
        </button>
        <button type="button" onClick={onRetry} style={BUTTON_STYLE}>
          {t("重试读取", "Retry")}
        </button>
        <button type="button" onClick={onRestore} style={DANGER_BUTTON_STYLE}>
          {t("恢复默认设置", "Restore defaults")}
        </button>
      </div>
    </div>
  );
}
