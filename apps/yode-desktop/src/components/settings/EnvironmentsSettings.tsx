import React, { useEffect, useState } from "react";
import { Folder, Plus, X, Trash2 } from "lucide-react";
import { loadDesktopSetting, saveDesktopSetting } from "../../lib/desktopSettings";

interface ProjectEnvironment {
  name: string;
  subtext?: string;
  path?: string;
  setupCommand?: string;
  execMode?: "host" | "docker" | "virtualenv";
  envVars?: Array<{ key: string; value: string }>;
}

export function EnvironmentsSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [projects, setProjects] = useState<ProjectEnvironment[]>(() => {
    const saved = localStorage.getItem("yode-environments-projects");
    if (saved) {
      try {
        return JSON.parse(saved);
      } catch (e) {
        // use default list
      }
    }
    return [
      { name: "langchain学习", setupCommand: "pip install -r requirements.txt", execMode: "virtualenv" },
      { name: "yode", subtext: "anYuJia", setupCommand: "cargo build", execMode: "host" },
      { name: "简历", setupCommand: "npm install && npm run build", execMode: "host" },
      { name: "11", subtext: "anpeny", execMode: "host" },
      { name: "lh", subtext: "anYuJia", execMode: "host" },
      { name: "douyin", execMode: "host" },
      { name: "datasearch", execMode: "host" },
      { name: "clear", execMode: "host" },
      { name: "syncFile", subtext: "anYuJia", execMode: "host" },
      { name: "26年5月", execMode: "host" },
      { name: "Easydict", subtext: "anYuJia", execMode: "host" }
    ];
  });

  const [expandedIndex, setExpandedIndex] = useState<number | null>(null);
  const [statusText, setStatusText] = useState("");

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [formName, setFormName] = useState("");
  const [formSubtext, setFormSubtext] = useState("");
  const [formPath, setFormPath] = useState("");

  const saveProjects = (list: ProjectEnvironment[]) => {
    setProjects(list);
    void saveDesktopSetting("yode-environments-projects", list);
  };

  useEffect(() => {
    void loadDesktopSetting("yode-environments-projects", projects).then(setProjects);
  }, []);

  const handleAddProject = () => {
    if (!formName.trim()) {
      setStatusText(t("项目名称不能为空。", "Project name cannot be empty."));
      return;
    }
    const newProj: ProjectEnvironment = {
      name: formName.trim(),
      subtext: formSubtext.trim() || undefined,
      path: formPath.trim() || undefined,
      execMode: "host",
      setupCommand: "",
      envVars: []
    };
    saveProjects([...projects, newProj]);
    setIsModalOpen(false);
    setFormName("");
    setFormSubtext("");
    setFormPath("");
    setStatusText(t("项目环境已添加。", "Project environment added."));
  };

  const handleDeleteProject = (index: number) => {
    if (confirm(t(`确定要删除项目 "${projects[index].name}" 吗？`, `Are you sure you want to delete project "${projects[index].name}"?`))) {
      const updated = projects.filter((_, i) => i !== index);
      saveProjects(updated);
      if (expandedIndex === index) {
        setExpandedIndex(null);
      } else if (expandedIndex !== null && expandedIndex > index) {
        setExpandedIndex(expandedIndex - 1);
      }
    }
  };

  const handleSaveEnvConfig = (index: number, updatedProj: ProjectEnvironment) => {
    const updated = projects.map((p, i) => (i === index ? updatedProj : p));
    saveProjects(updated);
    setStatusText(t("项目环境配置已保存。", "Project environment configuration saved."));
  };

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div style={{ fontSize: "12px", color: "var(--text-soft)", display: "flex", flexDirection: "column", gap: "4px" }}>
        <span>
          {t("本地环境告诉 Yode 如何为项目配置和拉起工作树。", "Local environments tell Yode how to set up worktrees for a project.")}{" "}
          <a href="#learn-environments" style={{ color: "var(--accent)", textDecoration: "none" }}>
            {t("了解更多", "Learn more.")}
          </a>
        </span>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
            {t("选择项目", "Select a project")}
          </span>
          <button
            onClick={() => setIsModalOpen(true)}
            type="button"
            className="secondary-button"
            style={{
              paddingInline: "12px",
              height: "28px",
              background: "var(--panel-raised)",
              borderColor: "var(--line)"
            }}
          >
            <span>{t("添加项目", "Add project")}</span>
          </button>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
          {projects.map((proj, idx) => {
            const isExpanded = expandedIndex === idx;
            return (
              <div
                key={idx}
                className="theme-card"
                style={{
                  padding: "0",
                  overflow: "hidden",
                  border: isExpanded ? "1px solid var(--accent)" : "1px solid var(--line-soft)",
                  transition: "border-color 150ms ease"
                }}
              >
                <div
                  onClick={() => setExpandedIndex(isExpanded ? null : idx)}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: "12px 16px",
                    cursor: "pointer",
                    background: isExpanded ? "var(--chrome)" : "transparent",
                    transition: "background 150ms ease"
                  }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
                    <Folder size={16} style={{ color: "var(--accent)", flexShrink: 0 }} />
                    <div style={{ display: "flex", alignItems: "baseline", gap: "8px" }}>
                      <span style={{ fontWeight: "600", fontSize: "13px", color: "var(--text)" }}>{proj.name}</span>
                      {proj.subtext && (
                        <span style={{ fontSize: "11.5px", color: "var(--text-soft)", opacity: 0.7 }}>{proj.subtext}</span>
                      )}
                    </div>
                  </div>

                  <button
                    type="button"
                    style={{
                      background: "var(--field)",
                      border: "1px solid var(--line-soft)",
                      borderRadius: "6px",
                      width: "24px",
                      height: "24px",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      color: "var(--text)",
                      cursor: "pointer",
                      padding: 0
                    }}
                  >
                    {isExpanded ? <X size={13} /> : <Plus size={13} />}
                  </button>
                </div>

                {isExpanded && (
                  <ExpandedProjectConfig
                    project={proj}
                    t={t}
                    isZh={isZh}
                    onSave={(updated) => handleSaveEnvConfig(idx, updated)}
                    onDelete={() => handleDeleteProject(idx)}
                  />
                )}
              </div>
            );
          })}
        </div>
        {statusText && (
          <div style={{ fontSize: "11px", color: "var(--text-soft)" }}>
            {statusText}
          </div>
        )}
      </div>

      {isModalOpen && (
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
              width: "400px",
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
              <span style={{ fontWeight: "600", fontSize: "13.5px", color: "var(--text)" }}>{t("添加本地项目", "Add Project")}</span>
              <button
                onClick={() => setIsModalOpen(false)}
                type="button"
                style={{ background: "transparent", border: "none", cursor: "pointer", color: "var(--text-soft)", display: "flex" }}
              >
                <X size={14} />
              </button>
            </div>

            <div style={{ padding: "16px", display: "flex", flexDirection: "column", gap: "12px" }}>
              <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                <label style={{ fontSize: "11px", color: "var(--text-soft)", fontWeight: "600" }}>{t("项目名称", "Project Name")}</label>
                <input
                  type="text"
                  value={formName}
                  onChange={(e) => setFormName(e.target.value)}
                  placeholder="e.g. langchain-study"
                  style={{
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "var(--radius)",
                    padding: "8px 12px",
                    fontSize: "12px",
                    color: "var(--text)",
                    outline: "none"
                  }}
                />
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                <label style={{ fontSize: "11px", color: "var(--text-soft)", fontWeight: "600" }}>
                  {t("子文本/拥有者 (可选)", "Owner/Subtext (Optional)")}
                </label>
                <input
                  type="text"
                  value={formSubtext}
                  onChange={(e) => setFormSubtext(e.target.value)}
                  placeholder="e.g. orgName"
                  style={{
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "var(--radius)",
                    padding: "8px 12px",
                    fontSize: "12px",
                    color: "var(--text)",
                    outline: "none"
                  }}
                />
              </div>

              <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                <label style={{ fontSize: "11px", color: "var(--text-soft)", fontWeight: "600" }}>
                  {t("本地绝对路径 (可选)", "Absolute Path (Optional)")}
                </label>
                <input
                  type="text"
                  value={formPath}
                  onChange={(e) => setFormPath(e.target.value)}
                  placeholder="e.g. /Users/username/code/project"
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
                />
              </div>
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
                onClick={() => setIsModalOpen(false)}
                type="button"
                style={{ background: "transparent", border: "none", fontSize: "12px", color: "var(--text-soft)", cursor: "pointer" }}
              >
                {t("取消", "Cancel")}
              </button>
              <button
                onClick={handleAddProject}
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

function ExpandedProjectConfig({
  project,
  t,
  isZh,
  onSave,
  onDelete
}: {
  project: ProjectEnvironment;
  t: (zh: string, en: string) => string;
  isZh: boolean;
  onSave: (proj: ProjectEnvironment) => void;
  onDelete: () => void;
}) {
  const [setupCmd, setSetupCmd] = useState(project.setupCommand || "");
  const [execMode, setExecMode] = useState<"host" | "docker" | "virtualenv">(project.execMode || "host");
  const [envVars, setEnvVars] = useState<Array<{ key: string; value: string }>>(() => {
    return project.envVars || [];
  });

  const handleAddEnv = () => {
    setEnvVars([...envVars, { key: "", value: "" }]);
  };

  const handleRemoveEnv = (idx: number) => {
    setEnvVars(envVars.filter((_, i) => i !== idx));
  };

  const handleEnvChange = (idx: number, field: "key" | "value", val: string) => {
    const updated = [...envVars];
    updated[idx][field] = val;
    setEnvVars(updated);
  };

  const handleSave = () => {
    const validEnv = envVars.filter((pair) => pair.key.trim().length > 0);
    onSave({
      ...project,
      setupCommand: setupCmd,
      execMode: execMode,
      envVars: validEnv
    });
  };

  return (
    <div
      style={{
        padding: "16px",
        background: "var(--panel-raised)",
        display: "flex",
        flexDirection: "column",
        gap: "16px",
        borderTop: "1px solid var(--line-soft)"
      }}
    >
      <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
        <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
          {t("环境构建/准备指令", "Environment Setup Command")}
        </label>
        <input
          type="text"
          value={setupCmd}
          onChange={(e) => setSetupCmd(e.target.value)}
          placeholder="e.g. npm install, cargo build, pip install -r requirements.txt"
          style={{
            background: "var(--field)",
            border: "1px solid var(--line-soft)",
            borderRadius: "var(--radius)",
            padding: "8px 12px",
            fontSize: "12.5px",
            color: "var(--text)",
            outline: "none",
            fontFamily: "var(--font-code)"
          }}
        />
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
        <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
          {t("脚本运行沙箱模式", "Sandbox / Execution Mode")}
        </label>
        <div className="segmented-control" style={{ maxWidth: "360px" }}>
          <button className={`segmented-btn ${execMode === "host" ? "active" : ""}`} onClick={() => setExecMode("host")} type="button">
            {t("主机 Shell", "Host")}
          </button>
          <button className={`segmented-btn ${execMode === "docker" ? "active" : ""}`} onClick={() => setExecMode("docker")} type="button">
            Docker
          </button>
          <button className={`segmented-btn ${execMode === "virtualenv" ? "active" : ""}`} onClick={() => setExecMode("virtualenv")} type="button">
            Virtualenv
          </button>
        </div>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <label style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase" }}>
            {t("项目环境变量", "Project Environment Variables")}
          </label>
          <button
            onClick={handleAddEnv}
            type="button"
            className="secondary-button"
            style={{
              fontSize: "10px",
              paddingInline: "8px",
              height: "20px",
              background: "var(--field)",
              borderColor: "var(--line-soft)"
            }}
          >
            {t("+ 添加", "+ Add")}
          </button>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
          {envVars.map((pair, idx) => (
            <div key={idx} style={{ display: "flex", gap: "8px", alignItems: "center" }}>
              <input
                type="text"
                placeholder="KEY"
                value={pair.key}
                onChange={(e) => handleEnvChange(idx, "key", e.target.value)}
                style={{
                  flex: 1,
                  background: "var(--field)",
                  border: "1px solid var(--line-soft)",
                  borderRadius: "var(--radius)",
                  padding: "6px 10px",
                  fontSize: "11.5px",
                  color: "var(--text)",
                  outline: "none",
                  fontFamily: "var(--font-code)"
                }}
              />
              <input
                type="text"
                placeholder="Value"
                value={pair.value}
                onChange={(e) => handleEnvChange(idx, "value", e.target.value)}
                style={{
                  flex: 1.5,
                  background: "var(--field)",
                  border: "1px solid var(--line-soft)",
                  padding: "6px 10px",
                  borderRadius: "var(--radius)",
                  fontSize: "11.5px",
                  color: "var(--text)",
                  outline: "none",
                  fontFamily: "var(--font-code)"
                }}
              />
              <button
                onClick={() => handleRemoveEnv(idx)}
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
                <Trash2 size={14} />
              </button>
            </div>
          ))}
          {envVars.length === 0 && (
            <span style={{ fontSize: "11px", color: "var(--text-soft)", opacity: 0.6, fontStyle: "italic" }}>
              {t("未配置环境变量", "No environment variables")}
            </span>
          )}
        </div>
      </div>

      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginTop: "8px",
          paddingTop: "12px",
          borderTop: "1px solid var(--line-soft)"
        }}
      >
        <button
          onClick={onDelete}
          type="button"
          style={{
            display: "flex",
            alignItems: "center",
            gap: "6px",
            background: "transparent",
            border: "1px solid rgba(224, 80, 80, 0.2)",
            borderRadius: "var(--radius)",
            padding: "6px 12px",
            fontSize: "12px",
            color: "oklch(67% 0.15 28)",
            cursor: "pointer",
            transition: "all 150ms ease"
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = "rgba(224, 80, 80, 0.1)";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = "transparent";
          }}
        >
          <Trash2 size={13} />
          <span>{t("删除项目", "Delete Project")}</span>
        </button>

        <button
          onClick={handleSave}
          type="button"
          style={{
            background: "var(--accent)",
            color: "var(--bg)",
            border: "none",
            borderRadius: "var(--radius)",
            padding: "6px 16px",
            fontSize: "12px",
            fontWeight: "600",
            cursor: "pointer"
          }}
        >
          {t("保存配置", "Save Config")}
        </button>
      </div>
    </div>
  );
}
