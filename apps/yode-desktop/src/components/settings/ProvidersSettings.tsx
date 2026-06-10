import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Search,
  Check,
  Eye,
  EyeOff,
  Plus,
  Trash2,
  Edit2,
  RefreshCw,
  AlertCircle,
  ToggleLeft,
  ToggleRight,
  PlusCircle,
  X,
  Cpu,
  HelpCircle,
  Globe,
  KeyRound,
  Bot
} from "lucide-react";
import { CustomSelect } from "../CustomSelect";

// Structure for provider config in localStorage
interface ProviderConfigData {
  id: string;
  name: string;
  format: "openai" | "gemini" | "anthropic";
  enabled: boolean;
  apiKey: string;
  baseUrl: string;
  models: string[];
  gradient?: string;
}

// Built-in presets to serve as starting templates or default added items
const PRESETS: Omit<ProviderConfigData, "enabled" | "apiKey">[] = [
  {
    id: "anthropic",
    name: "Anthropic Claude",
    format: "anthropic",
    baseUrl: "https://api.anthropic.com",
    models: ["claude-sonnet-4-20250514", "claude-opus-4-20250514", "claude-haiku-4-20250414"],
    gradient: "linear-gradient(135deg, #F27F52 0%, #D65A31 100%)"
  },
  {
    id: "openai",
    name: "OpenAI",
    format: "openai",
    baseUrl: "https://api.openai.com/v1",
    models: ["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "o1", "o1-mini", "o3-mini"],
    gradient: "linear-gradient(135deg, #10A37F 0%, #0D8566 100%)"
  },
  {
    id: "google",
    name: "Google Gemini",
    format: "gemini",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    models: ["gemini-2.5-pro", "gemini-2.5-flash", "gemini-2.0-flash"],
    gradient: "linear-gradient(135deg, #1A73E8 0%, #1557B0 100%)"
  },
  {
    id: "deepseek",
    name: "DeepSeek (深度求索)",
    format: "openai",
    baseUrl: "https://api.deepseek.com/v1",
    models: ["deepseek-chat", "deepseek-reasoner"],
    gradient: "linear-gradient(135deg, #4D77FF 0%, #2251F5 100%)"
  },
  {
    id: "ollama",
    name: "Ollama (本地运行)",
    format: "openai",
    baseUrl: "http://localhost:11434/v1",
    models: ["llama3.1", "qwen2.5-coder", "deepseek-coder-v2"],
    gradient: "linear-gradient(135deg, #666666 0%, #333333 100%)"
  }
];

export const PROVIDERS_META = PRESETS.map(p => ({
  id: p.id,
  name: p.name,
  nameEn: p.name,
  format: p.format,
  defaultBaseUrl: p.baseUrl,
  envKey: p.id.toUpperCase() + "_API_KEY",
  defaultModels: p.models,
  category: "recommend" as const,
  gradient: p.gradient || "linear-gradient(135deg, #3B82F6 0%, #1D4ED8 100%)"
}));

export function ProvidersSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [searchQuery, setSearchQuery] = useState("");
  const [providers, setProviders] = useState<ProviderConfigData[]>(() => {
    if ("__TAURI_INTERNALS__" in window) {
      return [];
    }
    const saved = localStorage.getItem("yode-llm-providers");
    if (saved) {
      try {
        const data = JSON.parse(saved);
        if (Array.isArray(data)) {
          return data;
        } else if (typeof data === "object") {
          // Migrate old object structure to new array structure
          return Object.keys(data).map((key) => {
            const preset = PRESETS.find(p => p.id === key);
            return {
              id: key,
              name: preset?.name || key.toUpperCase(),
              format: preset?.format || "openai",
              enabled: data[key].enabled,
              apiKey: data[key].apiKey || "",
              baseUrl: data[key].baseUrl || data[key].defaultBaseUrl || "",
              models: data[key].models || [],
              gradient: preset?.gradient || "linear-gradient(135deg, #6B7280 0%, #4B5563 100%)"
            };
          });
        }
      } catch (e) {
        // ignore
      }
    }
    // Default initial list
    return PRESETS.map((p) => ({
      ...p,
      enabled: p.id === "anthropic" || p.id === "openai" || p.id === "deepseek",
      apiKey: ""
    }));
  });

  useEffect(() => {
    if ("__TAURI_INTERNALS__" in window) {
      invoke<ProviderConfigData[]>("config_get_providers")
        .then((data) => {
          if (Array.isArray(data)) {
            setProviders(data);
            localStorage.setItem("yode-llm-providers", JSON.stringify(data));
          }
        })
        .catch(console.error);
    }
  }, []);

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [modalMode, setModalMode] = useState<"add" | "edit">("add");
  const [editingId, setEditingId] = useState<string | null>(null);

  // Form states
  const [formName, setFormName] = useState("");
  const [formFormat, setFormFormat] = useState<"openai" | "gemini" | "anthropic">("openai");
  const [formBaseUrl, setFormBaseUrl] = useState("");
  const [formApiKey, setFormApiKey] = useState("");
  const [formModels, setFormModels] = useState<string[]>([]);
  const [newModelInput, setNewModelInput] = useState("");
  const [visibleKey, setVisibleKey] = useState(false);

  // Testing status
  const [testingStatus, setTestingStatus] = useState<Record<string, "idle" | "testing" | "success" | "error">>({});

  const saveProviders = (list: ProviderConfigData[]) => {
    setProviders(list);
    localStorage.setItem("yode-llm-providers", JSON.stringify(list));
    if ("__TAURI_INTERNALS__" in window) {
      invoke("config_save_providers", { providers: list }).catch(console.error);
    }
  };

  const handleToggleProvider = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    const updated = providers.map((p) => (p.id === id ? { ...p, enabled: !p.enabled } : p));
    saveProviders(updated);
  };

  const handleDeleteProvider = (id: string, name: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (confirm(t(`确认要删除模型提供商 "${name}" 吗？`, `Are you sure you want to delete model provider "${name}"?`))) {
      const updated = providers.filter((p) => p.id !== id);
      saveProviders(updated);
    }
  };

  const openAddModal = () => {
    setModalMode("add");
    setEditingId(null);
    setFormName("");
    setFormFormat("openai");
    setFormBaseUrl("");
    setFormApiKey("");
    setFormModels([]);
    setNewModelInput("");
    setVisibleKey(false);
    setIsModalOpen(true);
  };

  const openEditModal = (provider: ProviderConfigData, e: React.MouseEvent) => {
    e.stopPropagation();
    setModalMode("edit");
    setEditingId(provider.id);
    setFormName(provider.name);
    setFormFormat(provider.format);
    setFormBaseUrl(provider.baseUrl);
    setFormApiKey(provider.apiKey);
    setFormModels([...provider.models]);
    setNewModelInput("");
    setVisibleKey(false);
    setIsModalOpen(true);
  };

  const handleAddModelTag = () => {
    const val = newModelInput.trim();
    if (val && !formModels.includes(val)) {
      setFormModels([...formModels, val]);
    }
    setNewModelInput("");
  };

  const handleRemoveModelTag = (val: string) => {
    setFormModels(formModels.filter((m) => m !== val));
  };

  const handleSaveProvider = () => {
    const name = formName.trim();
    if (!name) {
      alert(t("提供商名称不能为空", "Provider name cannot be empty"));
      return;
    }

    const id = editingId || name.toLowerCase().replace(/[^a-z0-9]/g, "-");
    
    // Check duplication for new ones
    if (modalMode === "add" && providers.some((p) => p.id === id)) {
      alert(t("该模型提供商已存在", "This model provider already exists"));
      return;
    }

    // Generate random gradient for custom providers
    const gradient =
      modalMode === "edit"
        ? providers.find((p) => p.id === id)?.gradient
        : `linear-gradient(135deg, hsl(${Math.floor(Math.random() * 360)}, 65%, 85%) 0%, hsl(${Math.floor(Math.random() * 360)}, 75%, 65%) 100%)`;

    const newProvider: ProviderConfigData = {
      id,
      name,
      format: formFormat,
      enabled: modalMode === "edit" ? (providers.find((p) => p.id === id)?.enabled ?? true) : true,
      apiKey: formApiKey,
      baseUrl: formBaseUrl.trim(),
      models: formModels,
      gradient: gradient || "linear-gradient(135deg, #6B7280 0%, #4B5563 100%)"
    };

    let updatedList: ProviderConfigData[];
    if (modalMode === "add") {
      updatedList = [...providers, newProvider];
    } else {
      updatedList = providers.map((p) => (p.id === id ? newProvider : p));
    }

    saveProviders(updatedList);
    setIsModalOpen(false);
  };

  const handleTestConnection = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setTestingStatus((prev) => ({ ...prev, [id]: "testing" }));
    const p = providers.find((p) => p.id === id);
    if (!p) {
      setTestingStatus((prev) => ({ ...prev, [id]: "error" }));
      return;
    }
    if ("__TAURI_INTERNALS__" in window) {
      try {
        await invoke("config_test_provider", { provider: p });
        setTestingStatus((prev) => ({ ...prev, [id]: "success" }));
      } catch (err) {
        console.error(err);
        setTestingStatus((prev) => ({ ...prev, [id]: "error" }));
      }
    } else {
      setTimeout(() => {
        const hasKey = p.apiKey.trim().length > 0 || p.id === "ollama";
        setTestingStatus((prev) => ({ ...prev, [id]: hasKey ? "success" : "error" }));
      }, 1500);
    }
  };

  const filteredProviders = providers.filter((p) =>
    p.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    p.id.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <div className="appearance-container" style={{ display: "flex", flexDirection: "column", gap: "20px" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <div style={{ fontSize: "12px", color: "var(--text-soft)", maxWidth: "80%" }}>
          {t(
            "管理并添加您的后端的模型提供商，您可以修改接口地址、更新密钥、调整可用模型，或创建自定义的 API 接口。",
            "Manage and add LLM providers for your backend. You can modify URLs, keys, models, or create custom API compatible integrations."
          )}
        </div>
        <button
          onClick={openAddModal}
          type="button"
          className="secondary-button"
          style={{
            display: "flex",
            alignItems: "center",
            gap: "6px",
            paddingInline: "12px",
            height: "28px",
            background: "var(--accent-muted)",
            color: "var(--text)",
            border: "1px solid var(--accent)"
          }}
        >
          <Plus size={14} />
          <span>{t("添加提供商", "Add Provider")}</span>
        </button>
      </div>

      {/* Search Bar */}
      <div style={{ position: "relative", width: "100%" }}>
        <Search
          size={13}
          style={{ position: "absolute", left: "9px", top: "8.5px", color: "var(--text-soft)", opacity: 0.8 }}
        />
        <input
          type="text"
          placeholder={t("搜索已添加的提供商...", "Search added providers...")}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{
            width: "100%",
            height: "28px",
            background: "var(--field)",
            border: "1px solid var(--line-soft)",
            borderRadius: "var(--radius)",
            paddingLeft: "28px",
            paddingRight: "8px",
            fontSize: "12px",
            color: "var(--text)",
            outline: "none"
          }}
        />
      </div>

      {/* Added list */}
      <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
        <span style={{ fontSize: "11px", fontWeight: "700", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px" }}>
          {t("已添加的提供商", "Added Providers")}
        </span>

        {filteredProviders.length === 0 ? (
          <div className="theme-card" style={{ padding: "32px 16px", textAlign: "center", color: "var(--text-soft)" }}>
            {t("暂无添加的模型提供商", "No added model providers")}
          </div>
        ) : (
          filteredProviders.map((p) => {
            const status = testingStatus[p.id] || "idle";
            return (
              <div
                key={p.id}
                className="theme-card provider-setting-card"
                style={{
                  border: "1px solid var(--line-soft)",
                  borderRadius: "var(--radius)",
                  background: "var(--panel)",
                  overflow: "hidden",
                  transition: "all 150ms ease"
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: "12px 16px",
                    minHeight: "56px"
                  }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
                    <div
                      style={{
                        width: "32px",
                        height: "32px",
                        borderRadius: "8px",
                        background: p.gradient || PRESETS.find(pr => pr.id === p.id || pr.format === p.format)?.gradient || "linear-gradient(135deg, #6B7280 0%, #4B5563 100%)",
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "center",
                        color: "#fff",
                        fontWeight: "700",
                        fontSize: "13px"
                      }}
                    >
                      {p.name.substring(0, 2).toUpperCase()}
                    </div>
                    <div style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
                      <span style={{ fontWeight: "650", fontSize: "13px", color: "var(--text)" }}>{p.name}</span>
                      <span style={{ fontSize: "10.5px", color: "var(--text-soft)", opacity: 0.8 }}>
                        Format: <code style={{ fontFamily: "var(--font-code)" }}>{p.format}</code> | URL: <code style={{ fontFamily: "var(--font-code)" }}>{p.baseUrl || "Default"}</code>
                      </span>
                    </div>
                  </div>

                  <div style={{ display: "flex", alignItems: "center", gap: "14px" }}>
                    {/* Status test feedback */}
                    <button
                      onClick={(e) => handleTestConnection(p.id, e)}
                      type="button"
                      className="secondary-button"
                      style={{
                        fontSize: "10.5px",
                        height: "22px",
                        paddingInline: "8px",
                        whiteSpace: "nowrap",
                        flexShrink: 0,
                        borderColor: status === "success" ? "rgba(80, 250, 123, 0.3)" : status === "error" ? "rgba(255, 85, 85, 0.3)" : "var(--line-soft)",
                        color: status === "success" ? "var(--success, #50FA7B)" : status === "error" ? "var(--error, #FF5555)" : "var(--text-soft)"
                      }}
                    >
                      {status === "testing" ? (
                        <RefreshCw size={11} className="spin" />
                      ) : (
                        <span>{status === "success" ? t("连接正常", "Success") : status === "error" ? t("连接失败", "Failed") : t("测试", "Test")}</span>
                      )}
                    </button>

                    {/* Edit button */}
                    <button
                      onClick={(e) => openEditModal(p, e)}
                      type="button"
                      title={t("编辑配置", "Edit configuration")}
                      style={{ background: "transparent", border: "none", cursor: "pointer", color: "var(--text-soft)" }}
                      onMouseEnter={(e) => (e.currentTarget.style.color = "var(--text)")}
                      onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-soft)")}
                    >
                      <Edit2 size={14} />
                    </button>

                    {/* Delete button */}
                    <button
                      onClick={(e) => handleDeleteProvider(p.id, p.name, e)}
                      type="button"
                      title={t("删除提供商", "Delete provider")}
                      style={{ background: "transparent", border: "none", cursor: "pointer", color: "var(--text-soft)" }}
                      onMouseEnter={(e) => (e.currentTarget.style.color = "oklch(67% 0.15 28)")}
                      onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-soft)")}
                    >
                      <Trash2 size={14} />
                    </button>

                    {/* Enable / Disable toggle */}
                    <button
                      onClick={(e) => handleToggleProvider(p.id, e)}
                      type="button"
                      style={{
                        background: "transparent",
                        border: "none",
                        cursor: "pointer",
                        color: p.enabled ? "var(--accent)" : "var(--text-soft)",
                        display: "flex",
                        alignItems: "center"
                      }}
                    >
                      {p.enabled ? <ToggleRight size={24} /> : <ToggleLeft size={24} />}
                    </button>
                  </div>
                </div>

                {/* Models summary display inline */}
                {p.models.length > 0 && (
                  <div style={{ padding: "0 16px 12px 60px", display: "flex", flexWrap: "wrap", gap: "6px" }}>
                    {p.models.map((model) => (
                      <span
                        key={model}
                        style={{
                          fontSize: "10px",
                          fontFamily: "var(--font-code)",
                          background: "var(--field)",
                          border: "1px solid var(--line-soft)",
                          color: "var(--text-muted)",
                          padding: "1px 6px",
                          borderRadius: "3px"
                        }}
                      >
                        {model}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>

      {/* Add / Edit Modal Overlay */}
      {isModalOpen && (
        <div
          style={{
            position: "fixed",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            background: "rgba(10, 10, 15, 0.65)",
            backdropFilter: "blur(12px)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 1000
          }}
        >
          <div
            className="theme-card"
            style={{
              width: "410px",
              maxHeight: "85vh",
              background: "var(--panel)",
              border: "1px solid var(--line-soft)",
              borderRadius: "12px",
              display: "flex",
              flexDirection: "column",
              boxShadow: "0 25px 50px -12px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.04) inset",
              overflow: "hidden"
            }}
          >
            {/* Modal Header */}
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                padding: "16px 20px 12px 20px",
                borderBottom: "1px solid rgba(255, 255, 255, 0.05)"
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                <Cpu size={15} style={{ color: "var(--accent)" }} />
                <span style={{ fontWeight: "600", fontSize: "14px", color: "var(--text)", letterSpacing: "-0.1px" }}>
                  {modalMode === "add" ? t("添加模型提供商", "Add Model Provider") : t("编辑模型提供商", "Edit Model Provider")}
                </span>
              </div>
              <button
                onClick={() => setIsModalOpen(false)}
                type="button"
                style={{
                  background: "transparent",
                  border: "none",
                  cursor: "pointer",
                  color: "var(--text-soft)",
                  display: "flex",
                  padding: "4px",
                  borderRadius: "4px",
                  transition: "background 150ms"
                }}
                onMouseEnter={(e) => (e.currentTarget.style.background = "rgba(255,255,255,0.06)")}
                onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
              >
                <X size={14} />
              </button>
            </div>

            {/* Modal Body */}
            <div style={{ padding: "16px 20px", overflowY: "auto", display: "flex", flexDirection: "column", gap: "12px", flex: 1 }}>
              {/* Name field */}
              <div style={{ display: "flex", flexDirection: "column", gap: "5px" }}>
                <label style={{ fontSize: "10px", fontWeight: "600", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px", display: "flex", alignItems: "center", gap: "4px" }}>
                  <Cpu size={10} />
                  {t("提供商名称", "Provider Name")}
                </label>
                <input
                  type="text"
                  value={formName}
                  onChange={(e) => setFormName(e.target.value)}
                  placeholder="e.g. Moonshot AI"
                  disabled={modalMode === "edit"}
                  style={{
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "6px",
                    padding: "6px 10px",
                    fontSize: "12px",
                    color: "var(--text)",
                    outline: "none",
                    transition: "border-color 150ms"
                  }}
                  onFocus={(e) => (e.currentTarget.style.borderColor = "var(--accent)")}
                  onBlur={(e) => (e.currentTarget.style.borderColor = "var(--line-soft)")}
                />
              </div>

              {/* Protocol format */}
              <div style={{ display: "flex", flexDirection: "column", gap: "5px" }}>
                <label style={{ fontSize: "10px", fontWeight: "600", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px", display: "flex", alignItems: "center", gap: "4px" }}>
                  <HelpCircle size={10} />
                  {t("协议格式", "Format")}
                </label>
                <CustomSelect
                  value={formFormat}
                  onChange={(val: any) => setFormFormat(val)}
                  options={[
                    { value: "openai", label: "OpenAI compatible", avatarText: "🤖" },
                    { value: "anthropic", label: "Anthropic compatible", avatarText: "🎨" },
                    { value: "gemini", label: "Gemini format", avatarText: "✨" }
                  ]}
                  style={{ width: "100%" }}
                />
              </div>

              {/* Base URL */}
              <div style={{ display: "flex", flexDirection: "column", gap: "5px" }}>
                <label style={{ fontSize: "10px", fontWeight: "600", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px", display: "flex", alignItems: "center", gap: "4px" }}>
                  <Globe size={10} />
                  {t("接口地址 (Base URL)", "API Base URL")}
                </label>
                <input
                  type="text"
                  value={formBaseUrl}
                  onChange={(e) => setFormBaseUrl(e.target.value)}
                  placeholder="e.g. https://api.moonshot.cn/v1"
                  style={{
                    background: "var(--field)",
                    border: "1px solid var(--line-soft)",
                    borderRadius: "6px",
                    padding: "6px 10px",
                    fontSize: "12px",
                    color: "var(--text)",
                    fontFamily: "var(--font-code)",
                    outline: "none",
                    transition: "border-color 150ms"
                  }}
                  onFocus={(e) => (e.currentTarget.style.borderColor = "var(--accent)")}
                  onBlur={(e) => (e.currentTarget.style.borderColor = "var(--line-soft)")}
                />
              </div>

              {/* API Key */}
              <div style={{ display: "flex", flexDirection: "column", gap: "5px" }}>
                <label style={{ fontSize: "10px", fontWeight: "600", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px", display: "flex", alignItems: "center", gap: "4px" }}>
                  <KeyRound size={10} />
                  API Key
                </label>
                <div style={{ position: "relative" }}>
                  <input
                    type={visibleKey ? "text" : "password"}
                    value={formApiKey}
                    onChange={(e) => setFormApiKey(e.target.value)}
                    placeholder={t("请输入 API 密钥", "Enter API Key")}
                    style={{
                      width: "100%",
                      background: "var(--field)",
                      border: "1px solid var(--line-soft)",
                      borderRadius: "6px",
                      paddingLeft: "10px",
                      paddingRight: "34px",
                      height: "28px",
                      fontSize: "12px",
                      color: "var(--text)",
                      fontFamily: visibleKey ? "var(--font-code)" : "password",
                      outline: "none",
                      transition: "border-color 150ms"
                    }}
                    onFocus={(e) => (e.currentTarget.style.borderColor = "var(--accent)")}
                    onBlur={(e) => (e.currentTarget.style.borderColor = "var(--line-soft)")}
                  />
                  <button
                    type="button"
                    onClick={() => setVisibleKey(!visibleKey)}
                    style={{
                      position: "absolute",
                      right: "8px",
                      top: "5px",
                      background: "transparent",
                      border: "none",
                      cursor: "pointer",
                      color: "var(--text-soft)"
                    }}
                  >
                    {visibleKey ? <EyeOff size={14} /> : <Eye size={14} />}
                  </button>
                </div>
              </div>

              {/* Models list */}
              <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                <label style={{ fontSize: "10px", fontWeight: "600", color: "var(--text-soft)", textTransform: "uppercase", letterSpacing: "0.5px", display: "flex", alignItems: "center", gap: "4px" }}>
                  <Bot size={10} />
                  {t("支持的模型列表", "Models")}
                </label>

                {/* Model tag container */}
                {formModels.length > 0 && (
                  <div style={{
                    display: "flex",
                    flexWrap: "wrap",
                    gap: "5px",
                    background: "var(--field)",
                    padding: "6px",
                    borderRadius: "6px",
                    border: "1px solid var(--line-soft)",
                    maxHeight: "80px",
                    overflowY: "auto"
                  }}>
                    {formModels.map((model) => (
                      <div
                        key={model}
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: "4px",
                          background: "color-mix(in oklch, var(--accent-muted), transparent 40%)",
                          border: "1px solid color-mix(in oklch, var(--accent), transparent 75%)",
                          borderRadius: "4px",
                          padding: "1px 6px",
                          fontSize: "10.5px",
                          fontFamily: "var(--font-code)",
                          color: "var(--text)",
                          transition: "all 150ms"
                        }}
                      >
                        <span>{model}</span>
                        <button
                          type="button"
                          onClick={() => handleRemoveModelTag(model)}
                          style={{
                            background: "transparent",
                            border: "none",
                            cursor: "pointer",
                            color: "var(--text-soft)",
                            padding: "0",
                            display: "flex",
                            alignItems: "center"
                          }}
                          onMouseEnter={(e) => (e.currentTarget.style.color = "oklch(67% 0.15 28)")}
                          onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-soft)")}
                        >
                          <X size={10} />
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                {/* Add model input */}
                <div style={{ display: "flex", gap: "6px" }}>
                  <input
                    type="text"
                    value={newModelInput}
                    onChange={(e) => setNewModelInput(e.target.value)}
                    placeholder={t("添加新模型, 如 kimi-k2.5", "Add new model, e.g. kimi-k2.5")}
                    style={{
                      flex: 1,
                      height: "26px",
                      background: "var(--field)",
                      border: "1px solid var(--line-soft)",
                      borderRadius: "6px",
                      paddingInline: "10px",
                      fontSize: "11px",
                      color: "var(--text)",
                      fontFamily: "var(--font-code)",
                      outline: "none"
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        handleAddModelTag();
                      }
                    }}
                  />
                  <button
                    onClick={handleAddModelTag}
                    type="button"
                    className="secondary-button"
                    style={{
                      height: "26px",
                      paddingInline: "10px",
                      fontSize: "11px",
                      background: "var(--accent-muted)",
                      color: "var(--text)",
                      borderColor: "var(--accent)",
                      fontWeight: "600"
                    }}
                  >
                    {t("添加", "Add")}
                  </button>
                </div>
              </div>
            </div>

            {/* Modal Footer */}
            <div
              style={{
                display: "flex",
                justifyContent: "flex-end",
                gap: "8px",
                padding: "12px 20px 16px 20px",
                borderTop: "1px solid rgba(255, 255, 255, 0.05)"
              }}
            >
              <button
                onClick={() => setIsModalOpen(false)}
                type="button"
                style={{
                  background: "transparent",
                  border: "none",
                  fontSize: "12px",
                  color: "var(--text-soft)",
                  cursor: "pointer",
                  padding: "4px 12px",
                  transition: "color 150ms"
                }}
                onMouseEnter={(e) => (e.currentTarget.style.color = "var(--text)")}
                onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-soft)")}
              >
                {t("取消", "Cancel")}
              </button>
              <button
                onClick={handleSaveProvider}
                type="button"
                style={{
                  background: "var(--accent)",
                  color: "var(--bg)",
                  border: "none",
                  borderRadius: "6px",
                  padding: "5px 16px",
                  fontSize: "12px",
                  fontWeight: "600",
                  cursor: "pointer",
                  transition: "opacity 150ms"
                }}
                onMouseEnter={(e) => (e.currentTarget.style.opacity = "0.9")}
                onMouseLeave={(e) => (e.currentTarget.style.opacity = "1")}
              >
                {t("保存", "Save")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
