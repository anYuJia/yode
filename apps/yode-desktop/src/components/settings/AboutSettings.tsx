import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ExternalLink, RefreshCw } from "lucide-react";
import { Bootstrap } from "../../lib/desktopTypes";

type UpdateCheckResult = {
  version: string;
  releaseUrl: string;
  publishedAt: string;
};

type UpdatePhase = "idle" | "checking" | "available" | "up_to_date" | "error";

export function AboutSettings({
  bootstrap,
  t
}: {
  bootstrap: Bootstrap;
  isZh?: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [phase, setPhase] = useState<UpdatePhase>("idle");
  const [updateInfo, setUpdateInfo] = useState<UpdateCheckResult | null>(null);
  const [statusText, setStatusText] = useState("");

  const checkForUpdates = async () => {
    if (!("__TAURI_INTERNALS__" in window)) {
      setPhase("error");
      setStatusText(t("仅在桌面应用中可用。", "Only available in the desktop app."));
      return;
    }
    setPhase("checking");
    setStatusText(t("正在检查更新...", "Checking for updates..."));
    setUpdateInfo(null);
    try {
      const result = await invoke<UpdateCheckResult | null>("check_for_updates");
      if (result) {
        setUpdateInfo(result);
        setPhase("available");
        setStatusText(
          t(
            `发现新版本 ${result.version}，请从官方 Release 页面下载安装。`,
            `Version ${result.version} is available. Download it from the official Release page.`
          )
        );
      } else {
        setPhase("up_to_date");
        setStatusText(t("当前已是最新版本。", "You are on the latest version."));
      }
    } catch (err) {
      console.error(err);
      setPhase("error");
      setStatusText(t("检查更新失败，请稍后重试。", "Failed to check for updates. Please try again."));
    }
  };

  const openReleaseUrl = () => {
    if (!updateInfo?.releaseUrl) return;
    window.open(updateInfo.releaseUrl, "_blank", "noopener,noreferrer");
  };

  return (
    <div className="appearance-container">
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
          {t("版本信息", "Version")}
        </span>
        <div className="theme-card">
          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("当前版本", "Current version")}</span>
              <span className="row-desc">
                {t("当前安装的 Yode 桌面端版本号", "Installed Yode desktop version")}
              </span>
            </div>
            <span
              style={{
                fontSize: "12px",
                fontFamily: "var(--font-code)",
                color: "var(--text-muted)",
                alignSelf: "center"
              }}
            >
              {bootstrap.appVersion || "—"}
            </span>
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
          {t("软件更新", "Software update")}
        </span>
        <div className="theme-card">
          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("检查更新", "Check for updates")}</span>
              <span className="row-desc">
                {t(
                  "从 GitHub Releases 查询最新桌面版本，不使用旧 CLI 二进制自更新器",
                  "Check GitHub Releases for the latest desktop build without the retired CLI binary updater"
                )}
              </span>
            </div>
            <button
              className="secondary-button"
              style={{
                display: "flex",
                alignItems: "center",
                gap: "6px",
                paddingInline: "12px",
                height: "28px"
              }}
              type="button"
              disabled={phase === "checking"}
              onClick={checkForUpdates}
            >
              <RefreshCw size={12} />
              <span>{phase === "checking" ? t("检查中", "Checking") : t("检查更新", "Check")}</span>
            </button>
          </div>

          {updateInfo && (
            <>
              <div className="divider" />
              <div className="form-row">
                <div className="row-info">
                  <span className="row-label">{t("最新版本", "Latest version")}</span>
                  <span className="row-desc">
                    {updateInfo.publishedAt
                      ? t(`发布于 ${updateInfo.publishedAt}`, `Published ${updateInfo.publishedAt}`)
                      : t("新的桌面版本可用", "A newer desktop release is available")}
                  </span>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                  <span
                    style={{
                      fontSize: "12px",
                      fontFamily: "var(--font-code)",
                      color: "var(--accent)",
                      alignSelf: "center"
                    }}
                  >
                    {updateInfo.version}
                  </span>
                  <button
                    className="secondary-button"
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: "6px",
                      paddingInline: "10px",
                      height: "28px"
                    }}
                    type="button"
                    onClick={openReleaseUrl}
                  >
                    <ExternalLink size={12} />
                    <span>{t("打开 Release", "Open Release")}</span>
                  </button>
                </div>
              </div>
            </>
          )}
        </div>

        {statusText && (
          <div style={{ fontSize: "11px", color: "var(--text-soft)", lineHeight: 1.5 }}>{statusText}</div>
        )}
      </div>
    </div>
  );
}
