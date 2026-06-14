import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, X, Globe } from "lucide-react";
import { CustomSelect } from "../CustomSelect";
import { isTauriRuntime, loadDesktopSetting, saveDesktopSetting } from "../../lib/desktopSettings";

export function BrowserSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [browserEnabled, setBrowserEnabled] = useState(() => {
    return localStorage.getItem("yode-browser-enabled") !== "false";
  });
  const [annotationScreenshots, setAnnotationScreenshots] = useState(() => {
    return localStorage.getItem("yode-browser-annotation-screenshots") || "Always include";
  });
  const [approvalPolicy, setApprovalPolicy] = useState(() => {
    return localStorage.getItem("yode-browser-approval") || "Always ask";
  });
  const [blockedDomains, setBlockedDomains] = useState<string[]>(() => {
    const saved = localStorage.getItem("yode-browser-blocked-domains");
    try {
      return saved ? JSON.parse(saved) : [];
    } catch (e) {
      return [];
    }
  });
  const [allowedDomains, setAllowedDomains] = useState<string[]>(() => {
    const saved = localStorage.getItem("yode-browser-allowed-domains");
    try {
      return saved ? JSON.parse(saved) : [];
    } catch (e) {
      return [];
    }
  });

  const [domainModalType, setDomainModalType] = useState<"blocked" | "allowed" | null>(null);
  const [newDomainInput, setNewDomainInput] = useState("");
  const [statusText, setStatusText] = useState("");

  useEffect(() => {
    void loadDesktopSetting("yode-browser-enabled", browserEnabled).then(setBrowserEnabled);
    void loadDesktopSetting("yode-browser-annotation-screenshots", annotationScreenshots).then(setAnnotationScreenshots);
    void loadDesktopSetting("yode-browser-approval", approvalPolicy).then(setApprovalPolicy);
    void loadDesktopSetting("yode-browser-blocked-domains", blockedDomains).then(setBlockedDomains);
    void loadDesktopSetting("yode-browser-allowed-domains", allowedDomains).then(setAllowedDomains);
  }, []);

  const saveBlocked = (list: string[]) => {
    setBlockedDomains(list);
    void saveDesktopSetting("yode-browser-blocked-domains", list);
  };

  const saveAllowed = (list: string[]) => {
    setAllowedDomains(list);
    void saveDesktopSetting("yode-browser-allowed-domains", list);
  };

  const handleAddDomain = () => {
    const domain = newDomainInput.trim().toLowerCase();
    if (!domain) return;
    if (domainModalType === "blocked") {
      if (!blockedDomains.includes(domain)) {
        saveBlocked([...blockedDomains, domain]);
      }
    } else if (domainModalType === "allowed") {
      if (!allowedDomains.includes(domain)) {
        saveAllowed([...allowedDomains, domain]);
      }
    }
    setNewDomainInput("");
    setDomainModalType(null);
  };

  const handleRemoveDomain = (type: "blocked" | "allowed", domain: string) => {
    if (type === "blocked") {
      saveBlocked(blockedDomains.filter((d) => d !== domain));
    } else {
      saveAllowed(allowedDomains.filter((d) => d !== domain));
    }
  };

  const handleClearBrowsingData = async () => {
    if (!isTauriRuntime()) {
      setStatusText(t("浏览器数据会在桌面端清理。", "Browsing data is cleared in the desktop runtime."));
      return;
    }
    try {
      const result = await invoke<{ ok: boolean; message: string }>("browser_clear_data");
      setStatusText(result.message);
    } catch (err) {
      console.error(err);
      setStatusText(t("清除浏览数据失败。", "Failed to clear browsing data."));
    }
  };

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div style={{ fontSize: "12px", color: "var(--text-soft)", display: "flex", flexDirection: "column", gap: "2px" }}>
        <span>
          {t("管理 Yode 的浏览器。Google Chrome 可以在", "Manage Yode's browser. Google Chrome can be set up in")}{" "}
          <a href="#computer-use" style={{ color: "var(--accent)", textDecoration: "none" }}>
            {t("计算机使用设置", "computer use settings")}
          </a>{" "}
          {t("中进行配置。", "settings.")}
        </span>
      </div>

      <div className="theme-card" style={{ padding: "12px 16px" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
            <div
              style={{
                width: "32px",
                height: "32px",
                borderRadius: "var(--radius)",
                background: "var(--field)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: "var(--accent)"
              }}
            >
              <Globe size={18} />
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
              <span style={{ fontWeight: "650", fontSize: "13px", color: "var(--text)" }}>{t("浏览器", "Browser")}</span>
              <span style={{ fontSize: "11px", color: "var(--text-soft)" }}>{t("允许 Yode 控制内置浏览器", "Let Yode control the built-in browser")}</span>
            </div>
          </div>
          <label className="switch-wrapper">
            <input
              type="checkbox"
              checked={browserEnabled}
              onChange={(e) => {
                setBrowserEnabled(e.target.checked);
                void saveDesktopSetting("yode-browser-enabled", e.target.checked);
              }}
            />
            <span className="switch-slider" />
          </label>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
          {t("数据管理", "Data")}
        </span>
        <div className="theme-card">
          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("浏览数据", "Browsing data")}</span>
              <span className="row-desc">{t("清除应用内浏览器的网站数据 and 缓存", "Clear site data and cache from the in-app browser")}</span>
            </div>
            <button
              onClick={handleClearBrowsingData}
              type="button"
              className="secondary-button"
              style={{ paddingInline: "12px", height: "26px", fontSize: "11.5px" }}
            >
              {t("清除所有浏览数据", "Clear all browsing data")}
            </button>
          </div>

          <div className="divider" />

          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("网页标注截图", "Annotation screenshots")}</span>
              <span className="row-desc">
                {t(
                  "截图能帮助 Yode 更好地理解和解决内容问题，但会增加 Token 用量",
                  "Screenshots help Yode better understand and address comments, but increase plan usage"
                )}
              </span>
            </div>
            <CustomSelect
              value={annotationScreenshots}
              onChange={(val) => {
                setAnnotationScreenshots(val);
                void saveDesktopSetting("yode-browser-annotation-screenshots", val);
              }}
              options={[
                { value: "Always include", label: t("总是包含", "Always include") },
                { value: "Never include", label: t("从不包含", "Never include") },
                { value: "Ask each time", label: t("每次询问", "Ask each time") }
              ]}
              style={{ minWidth: "150px" }}
            />
          </div>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
          {t("权限与审批", "Permissions")}
        </span>
        <div className="theme-card">
          <div className="form-row">
            <div className="row-info">
              <span className="row-label">{t("授权审批", "Approval")}</span>
              <span className="row-desc">
                {t("选择 Yode 在打开网站前是否需要征求您的同意。", "Choose if Yode asks for approval before opening websites.")}{" "}
                <a href="#learn-approval" style={{ color: "var(--accent)", textDecoration: "none" }}>
                  {t("了解更多", "Learn more")}
                </a>
              </span>
            </div>
            <CustomSelect
              value={approvalPolicy}
              onChange={(val) => {
                setApprovalPolicy(val);
                void saveDesktopSetting("yode-browser-approval", val);
              }}
              options={[
                { value: "Always ask", label: t("总是询问", "Always ask") },
                { value: "Always allow", label: t("总是允许", "Always allow") },
                { value: "Never allow", label: t("从不允许", "Never allow") }
              ]}
              style={{ minWidth: "150px" }}
            />
          </div>
        </div>
        {statusText && (
          <div style={{ fontSize: "11px", color: "var(--text-soft)" }}>
            {statusText}
          </div>
        )}
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("已拦截域名", "Blocked domains")}
          </span>
          <button
            onClick={() => setDomainModalType("blocked")}
            type="button"
            className="secondary-button"
            style={{
              display: "flex",
              alignItems: "center",
              gap: "4px",
              paddingInline: "10px",
              height: "22px",
              fontSize: "11px"
            }}
          >
            <Plus size={12} />
            <span>{t("添加", "Add")}</span>
          </button>
        </div>
        <span style={{ fontSize: "11px", color: "var(--text-soft)", marginTop: "-4px" }}>
          {t("Yode 将永远不会打开这些网站", "Yode will never open these sites")}
        </span>
        <div className="theme-card" style={{ padding: blockedDomains.length > 0 ? "4px 0" : "16px 12px" }}>
          {blockedDomains.length === 0 ? (
            <div style={{ textAlign: "center", fontSize: "12px", color: "var(--text-soft)", opacity: 0.7 }}>
              {t("暂无被拦截的域名", "No blocked domains")}
            </div>
          ) : (
            blockedDomains.map((domain, idx) => (
              <div key={domain}>
                {idx > 0 && <div className="divider" style={{ margin: "2px 16px" }} />}
                <div className="form-row" style={{ minHeight: "36px", paddingBlock: "4px" }}>
                  <span style={{ fontFamily: "var(--font-code)", fontSize: "12.5px" }}>{domain}</span>
                  <button
                    onClick={() => handleRemoveDomain("blocked", domain)}
                    type="button"
                    style={{
                      background: "transparent",
                      border: "none",
                      cursor: "pointer",
                      color: "var(--text-soft)",
                      padding: "4px"
                    }}
                    onMouseEnter={(e) => (e.currentTarget.style.color = "oklch(67% 0.15 28)")}
                    onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-soft)")}
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("已允许域名", "Allowed domains")}
          </span>
          <button
            onClick={() => setDomainModalType("allowed")}
            type="button"
            className="secondary-button"
            style={{
              display: "flex",
              alignItems: "center",
              gap: "4px",
              paddingInline: "10px",
              height: "22px",
              fontSize: "11px"
            }}
          >
            <Plus size={12} />
            <span>{t("添加", "Add")}</span>
          </button>
        </div>
        <span style={{ fontSize: "11px", color: "var(--text-soft)", marginTop: "-4px" }}>
          {t("无需询问即可直接打开的域名", "Domains that open without asking")}
        </span>
        <div className="theme-card" style={{ padding: allowedDomains.length > 0 ? "4px 0" : "16px 12px" }}>
          {allowedDomains.length === 0 ? (
            <div style={{ textAlign: "center", fontSize: "12px", color: "var(--text-soft)", opacity: 0.7 }}>
              {t("暂无自动允许的域名", "No allowed domains")}
            </div>
          ) : (
            allowedDomains.map((domain, idx) => (
              <div key={domain}>
                {idx > 0 && <div className="divider" style={{ margin: "2px 16px" }} />}
                <div className="form-row" style={{ minHeight: "36px", paddingBlock: "4px" }}>
                  <span style={{ fontFamily: "var(--font-code)", fontSize: "12.5px" }}>{domain}</span>
                  <button
                    onClick={() => handleRemoveDomain("allowed", domain)}
                    type="button"
                    style={{
                      background: "transparent",
                      border: "none",
                      cursor: "pointer",
                      color: "var(--text-soft)",
                      padding: "4px"
                    }}
                    onMouseEnter={(e) => (e.currentTarget.style.color = "oklch(67% 0.15 28)")}
                    onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-soft)")}
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>

      {domainModalType !== null && (
        <div
          style={{
            position: "fixed",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            background: "rgba(0, 0, 0, 0.65)",
            backdropFilter: "blur(6px)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 1000
          }}
        >
          <div
            className="theme-card"
            style={{
              width: "360px",
              background: "var(--panel)",
              border: "1px solid var(--line)",
              borderRadius: "var(--radius)",
              display: "flex",
              flexDirection: "column",
              boxShadow: "0 20px 25px -5px rgb(0 0 0 / 0.3)"
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                padding: "12px 16px",
                borderBottom: "1px solid var(--line-soft)",
                background: "var(--chrome)"
              }}
            >
              <span style={{ fontWeight: "600", fontSize: "13px", color: "var(--text)" }}>
                {domainModalType === "blocked" ? t("拦截新域名", "Block Domain") : t("允许新域名", "Allow Domain")}
              </span>
              <button
                onClick={() => setDomainModalType(null)}
                type="button"
                style={{ background: "transparent", border: "none", cursor: "pointer", color: "var(--text-soft)", display: "flex" }}
              >
                <X size={14} />
              </button>
            </div>

            <div style={{ padding: "16px", display: "flex", flexDirection: "column", gap: "12px" }}>
              <input
                type="text"
                value={newDomainInput}
                onChange={(e) => setNewDomainInput(e.target.value)}
                placeholder="e.g. example.com"
                style={{
                  background: "var(--field)",
                  border: "1px solid var(--line-soft)",
                  borderRadius: "var(--radius)",
                  padding: "8px 12px",
                  fontSize: "12px",
                  color: "var(--text)",
                  outline: "none",
                  fontFamily: "var(--font-code)"
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleAddDomain();
                }}
              />
            </div>

            <div
              style={{
                display: "flex",
                justifyContent: "flex-end",
                gap: "8px",
                padding: "10px 16px",
                borderTop: "1px solid var(--line-soft)",
                background: "var(--chrome)"
              }}
            >
              <button
                onClick={() => setDomainModalType(null)}
                type="button"
                style={{ background: "transparent", border: "none", fontSize: "12px", color: "var(--text-soft)", cursor: "pointer" }}
              >
                {t("取消", "Cancel")}
              </button>
              <button
                onClick={handleAddDomain}
                type="button"
                style={{
                  background: "var(--accent)",
                  color: "var(--bg)",
                  border: "none",
                  borderRadius: "var(--radius)",
                  padding: "4px 12px",
                  fontSize: "12px",
                  fontWeight: "600",
                  cursor: "pointer"
                }}
              >
                {t("添加", "Add")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
