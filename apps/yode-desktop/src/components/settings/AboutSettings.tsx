import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Download, RefreshCw, RotateCcw, ExternalLink } from "lucide-react";
import { Bootstrap } from "../../lib/desktopTypes";

type UpdateCheckResult = {
  version: string;
  releaseUrl: string;
  publishedAt: string;
};

type UpdatePhase =
  | "idle"
  | "checking"
  | "available"
  | "up_to_date"
  | "downloading"
  | "ready"
  | "applying"
  | "error";

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
  const [downloadPath, setDownloadPath] = useState("");

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    invoke<boolean>("has_pending_update")
      .then((pending) => {
        if (pending) {
          setPhase("ready");
          setStatusText(t("已下载更新，可立即应用并重启。", "An update is ready to apply and restart."));
        }
      })
      .catch(console.error);
  }, [t]);

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
            `发现新版本 ${result.version}。`,
            `New version ${result.version} is available.`
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

  const downloadUpdate = async () => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    setPhase("downloading");
    setStatusText(t("正在下载更新...", "Downloading update..."));
    try {
      const path = await invoke<string>("download_update");
      setDownloadPath(path);
      setPhase("ready");
      setStatusText(t("下载完成，可立即应用并重启。", "Download complete. Ready to apply and restart."));
    } catch (err) {
      console.error(err);
      setPhase("error");
      setStatusText(t("下载更新失败，请稍后重试。", "Failed to download update. Please try again."));
    }
  };

  const applyAndRestart = async () => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    setPhase("applying");
    setStatusText(t("正在应用更新并重启...", "Applying update and restarting..."));
    try {
      const applied = await invoke<boolean>("apply_downloaded_update");
      if (!applied) {
        setPhase("error");
        setStatusText(t("没有可应用的更新包。", "No downloaded update package to apply."));
        return;
      }
      await invoke("app_restart");
    } catch (err) {
      console.error(err);
      setPhase("error");
      setStatusText(t("应用更新失败，请稍后重试。", "Failed to apply update. Please try again."));
    }
  };

  const openReleaseUrl = () => {
    if (!updateInfo?.releaseUrl) return;
    window.open(updateInfo.releaseUrl, "_blank", "noopener,noreferrer");
  };

  const busy = phase === "checking" || phase === "downloading" || phase === "applying";

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
              <span className="row-desc">{t("当前安装的 Yode 桌面端版本号", "Installed Yode desktop version")}</span>
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
                {t("从 GitHub Releases 查询是否有新版本", "Query GitHub Releases for a newer build")}
              </span>
            </div>
            <button
              className="secondary-button"
              style={{ display: "flex", alignItems: "center", gap: "6px", paddingInline: "12px", height: "28px" }}
              type="button"
              disabled={busy}
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
                      : t("可下载的新版本", "A newer release is available")}
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
                  {updateInfo.releaseUrl && (
                    <button
                      className="icon-button"
                      type="button"
                      onClick={openReleaseUrl}
                      title={t("打开发布页", "Open release page")}
                      aria-label={t("打开发布页", "Open release page")}
                    >
                      <ExternalLink size={14} />
                    </button>
                  )}
                </div>
              </div>
            </>
          )}

          {(phase === "available" || phase === "downloading") && (
            <>
              <div className="divider" />
              <div className="form-row">
                <div className="row-info">
                  <span className="row-label">{t("下载更新", "Download update")}</span>
                  <span className="row-desc">{t("下载安装包到本地缓存", "Download the package into the local cache")}</span>
                </div>
                <button
                  className="secondary-button"
                  style={{ display: "flex", alignItems: "center", gap: "6px", paddingInline: "12px", height: "28px" }}
                  type="button"
                  disabled={busy}
                  onClick={downloadUpdate}
                >
                  <Download size={12} />
                  <span>{phase === "downloading" ? t("下载中", "Downloading") : t("下载", "Download")}</span>
                </button>
              </div>
            </>
          )}

          {(phase === "ready" || phase === "applying") && (
            <>
              <div className="divider" />
              <div className="form-row">
                <div className="row-info">
                  <span className="row-label">{t("应用并重启", "Apply and restart")}</span>
                  <span className="row-desc">
                    {t("替换当前安装并重启应用以完成更新", "Replace the current install and restart to finish")}
                  </span>
                </div>
                <button
                  className="secondary-button"
                  style={{ display: "flex", alignItems: "center", gap: "6px", paddingInline: "12px", height: "28px" }}
                  type="button"
                  disabled={busy}
                  onClick={applyAndRestart}
                >
                  <RotateCcw size={12} />
                  <span>{phase === "applying" ? t("应用中", "Applying") : t("应用并重启", "Apply & restart")}</span>
                </button>
              </div>
            </>
          )}
        </div>

        {statusText && (
          <div style={{ fontSize: "11px", color: "var(--text-soft)", lineHeight: 1.5 }}>{statusText}</div>
        )}
        {downloadPath && (
          <div style={{ fontSize: "11px", color: "var(--text-soft)", fontFamily: "var(--font-code)", lineHeight: 1.5 }}>
            {downloadPath}
          </div>
        )}
      </div>
    </div>
  );
}
