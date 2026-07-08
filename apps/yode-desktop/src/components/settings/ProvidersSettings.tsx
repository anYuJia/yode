import React, { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertCircle,
  Bot,
  ChevronDown,
  Check,
  Edit2,
  Eye,
  EyeOff,
  Globe,
  KeyRound,
  Plus,
  Search,
  ShieldCheck,
  Trash2,
  X
} from "lucide-react";
import { CustomSelect } from "../CustomSelect";
import { Bootstrap, DefaultLlm } from "../../lib/desktopTypes";
import {
  dispatchDefaultLlmChange,
  loadStoredProviderValues,
  saveLastModelForProvider,
  saveStoredProviders
} from "../../lib/llmProviderStorage";
import { recordFromUnknown } from "../../lib/jsonUtils";

interface ProviderConfigData {
  id: string;
  name: string;
  format: "openai" | "gemini" | "anthropic";
  enabled: boolean;
  apiKey: string;
  baseUrl: string;
  models: string[];
  gradient?: string;
  icon?: string;
}

function isProviderFormat(value: string): value is ProviderConfigData["format"] {
  return value === "openai" || value === "gemini" || value === "anthropic";
}

type ProviderTemplate = Omit<ProviderConfigData, "enabled" | "apiKey"> & {
  note: string;
  group: "global" | "china" | "gateway" | "local";
};

const BUILT_IN_PROVIDERS: ProviderTemplate[] = [
  {
    id: "openai",
    name: "OpenAI",
    format: "openai",
    baseUrl: "https://api.openai.com/v1",
    models: ["gpt-5.5", "gpt-5.4", "gpt-5.4-mini", "gpt-5.4-nano", "gpt-4.1"],
    gradient: "linear-gradient(135deg, #109A7A 0%, #0C735D 100%)",
    note: "官方接口",
    group: "global"
  },
  {
    id: "anthropic",
    name: "Anthropic Claude",
    format: "anthropic",
    baseUrl: "https://api.anthropic.com",
    models: ["claude-fable-5", "claude-opus-4-8", "claude-sonnet-4-6", "claude-haiku-4-5-20251001"],
    gradient: "linear-gradient(135deg, #D97742 0%, #A94D2E 100%)",
    note: "Messages API",
    group: "global"
  },
  {
    id: "gemini",
    name: "Google Gemini",
    format: "gemini",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    models: ["gemini-3.1-pro-preview", "gemini-3.5-flash", "gemini-3-flash-preview", "gemini-3.1-flash-lite"],
    gradient: "linear-gradient(135deg, #3C82F6 0%, #2561C9 100%)",
    note: "Google AI Studio",
    group: "global"
  },
  {
    id: "azure-openai",
    name: "Azure OpenAI",
    format: "openai",
    baseUrl: "https://{resource}.openai.azure.com/openai/deployments/{deployment}",
    models: ["gpt-5.5", "gpt-5.4", "gpt-5.4-mini", "gpt-4.1"],
    gradient: "linear-gradient(135deg, #3277C8 0%, #1C4E8C 100%)",
    note: "Azure 部署地址",
    group: "global"
  },
  {
    id: "mistral",
    name: "Mistral AI",
    format: "openai",
    baseUrl: "https://api.mistral.ai/v1",
    models: ["mistral-large-latest", "codestral-latest", "ministral-8b-latest"],
    gradient: "linear-gradient(135deg, #D76F35 0%, #8D4227 100%)",
    note: "OpenAI 兼容",
    group: "global"
  },
  {
    id: "xai",
    name: "xAI",
    format: "openai",
    baseUrl: "https://api.x.ai/v1",
    models: ["grok-4", "grok-4-fast", "grok-3", "grok-3-mini"],
    gradient: "linear-gradient(135deg, #5E6673 0%, #2F3742 100%)",
    note: "OpenAI 兼容",
    group: "global"
  },
  {
    id: "groq",
    name: "Groq",
    format: "openai",
    baseUrl: "https://api.groq.com/openai/v1",
    models: ["openai/gpt-oss-120b", "llama-3.3-70b-versatile", "deepseek-r1-distill-llama-70b"],
    gradient: "linear-gradient(135deg, #C55A4D 0%, #7F3832 100%)",
    note: "高速推理",
    group: "global"
  },
  {
    id: "together",
    name: "Together AI",
    format: "openai",
    baseUrl: "https://api.together.xyz/v1",
    models: ["meta-llama/Llama-3.3-70B-Instruct-Turbo", "Qwen/Qwen2.5-Coder-32B-Instruct"],
    gradient: "linear-gradient(135deg, #7568D8 0%, #4D428F 100%)",
    note: "开源模型平台",
    group: "gateway"
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    format: "openai",
    baseUrl: "https://openrouter.ai/api/v1",
    models: ["openai/gpt-5.5", "anthropic/claude-sonnet-4.6", "google/gemini-3.1-pro-preview"],
    gradient: "linear-gradient(135deg, #6E7581 0%, #404752 100%)",
    note: "多模型路由",
    group: "gateway"
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    format: "openai",
    baseUrl: "https://api.deepseek.com/v1",
    models: ["deepseek-v4-pro", "deepseek-v4-flash", "deepseek-chat", "deepseek-reasoner"],
    gradient: "linear-gradient(135deg, #5F7EF2 0%, #344FC7 100%)",
    note: "官方兼容接口",
    group: "china"
  },
  {
    id: "moonshot",
    name: "Moonshot AI",
    format: "openai",
    baseUrl: "https://api.moonshot.cn/v1",
    models: ["kimi-k2", "moonshot-v1-128k", "moonshot-v1-32k", "moonshot-v1-8k"],
    gradient: "linear-gradient(135deg, #8B7CF6 0%, #5B4BC4 100%)",
    note: "Kimi / Moonshot",
    group: "china"
  },
  {
    id: "qwen",
    name: "通义千问",
    format: "openai",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    models: ["qwen3-max", "qwen3-coder-plus", "qwen-plus-latest", "qwen-turbo-latest"],
    gradient: "linear-gradient(135deg, #E06B3D 0%, #B44429 100%)",
    note: "DashScope 兼容模式",
    group: "china"
  },
  {
    id: "doubao",
    name: "豆包",
    format: "openai",
    baseUrl: "https://ark.cn-beijing.volces.com/api/v3",
    models: ["doubao-seed-1-6", "doubao-seed-1-6-thinking", "doubao-1-5-pro-32k"],
    gradient: "linear-gradient(135deg, #2CA58D 0%, #1F7567 100%)",
    note: "火山方舟",
    group: "china"
  },
  {
    id: "zhipu",
    name: "智谱 GLM",
    format: "openai",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    models: ["glm-4.6", "glm-4-plus", "glm-z1-air", "glm-4-flash"],
    gradient: "linear-gradient(135deg, #3D8C91 0%, #285E66 100%)",
    note: "BigModel 兼容",
    group: "china"
  },
  {
    id: "baichuan",
    name: "百川智能",
    format: "openai",
    baseUrl: "https://api.baichuan-ai.com/v1",
    models: ["Baichuan4-Turbo", "Baichuan4", "Baichuan3-Turbo"],
    gradient: "linear-gradient(135deg, #4C7AA7 0%, #31506E 100%)",
    note: "OpenAI 兼容",
    group: "china"
  },
  {
    id: "minimax",
    name: "MiniMax",
    format: "openai",
    baseUrl: "https://api.minimax.chat/v1",
    models: ["MiniMax-M1", "MiniMax-Text-01", "abab6.5s-chat"],
    gradient: "linear-gradient(135deg, #B46A78 0%, #77434D 100%)",
    note: "OpenAI 兼容",
    group: "china"
  },
  {
    id: "baidu-qianfan",
    name: "百度千帆",
    format: "anthropic",
    baseUrl: "https://qianfan.baidubce.com/anthropic/coding",
    models: ["qianfan-code-latest"],
    gradient: "linear-gradient(135deg, #4C78BD 0%, #2E5188 100%)",
    note: "Anthropic 兼容",
    group: "china"
  },
  {
    id: "dashscope-coding",
    name: "阿里 Coding",
    format: "anthropic",
    baseUrl: "https://coding.dashscope.aliyuncs.com/apps/anthropic",
    models: ["qwen3.5-plus"],
    gradient: "linear-gradient(135deg, #C97042 0%, #8F442B 100%)",
    note: "Anthropic 兼容",
    group: "china"
  },
  {
    id: "siliconflow",
    name: "SiliconFlow",
    format: "openai",
    baseUrl: "https://api.siliconflow.cn/v1",
    models: ["deepseek-ai/DeepSeek-V4-Pro", "deepseek-ai/DeepSeek-V4-Flash", "Qwen/Qwen3-Coder"],
    gradient: "linear-gradient(135deg, #6B8B3F 0%, #48622D 100%)",
    note: "国内聚合平台",
    group: "gateway"
  },
  {
    id: "302-ai",
    name: "302.AI",
    format: "openai",
    baseUrl: "https://api.302.ai/v1",
    models: ["gpt-5.5", "claude-sonnet-4-6", "gemini-3.1-pro-preview", "deepseek-v4-pro"],
    gradient: "linear-gradient(135deg, #7A7092 0%, #504961 100%)",
    note: "多模型聚合",
    group: "gateway"
  },
  {
    id: "oneapi",
    name: "One API",
    format: "openai",
    baseUrl: "http://localhost:3000/v1",
    models: ["gpt-5.5", "claude-sonnet-4-6", "deepseek-v4-pro", "qwen3-coder-plus"],
    gradient: "linear-gradient(135deg, #5D7280 0%, #394753 100%)",
    note: "自建聚合网关",
    group: "gateway"
  },
  {
    id: "new-api",
    name: "New API",
    format: "openai",
    baseUrl: "http://localhost:3000/v1",
    models: ["gpt-5.5", "deepseek-v4-pro", "qwen3-coder-plus", "gemini-3.1-pro-preview"],
    gradient: "linear-gradient(135deg, #657882 0%, #43515A 100%)",
    note: "自建聚合网关",
    group: "gateway"
  },
  {
    id: "lmstudio",
    name: "LM Studio",
    format: "openai",
    baseUrl: "http://localhost:1234/v1",
    models: ["local-model"],
    gradient: "linear-gradient(135deg, #6A7580 0%, #424A53 100%)",
    note: "本地 OpenAI 兼容",
    group: "local"
  },
  {
    id: "vllm",
    name: "vLLM",
    format: "openai",
    baseUrl: "http://localhost:8000/v1",
    models: ["local-model"],
    gradient: "linear-gradient(135deg, #6D766C 0%, #454E44 100%)",
    note: "自部署推理服务",
    group: "local"
  },
  {
    id: "ollama",
    name: "Ollama",
    format: "openai",
    baseUrl: "http://localhost:11434/v1",
    models: ["llama3.1", "qwen2.5-coder", "deepseek-coder-v2"],
    gradient: "linear-gradient(135deg, #6B7280 0%, #3F4650 100%)",
    note: "本地模型",
    group: "local"
  }
];

const PROVIDER_GROUPS: Array<{ id: ProviderTemplate["group"]; labelZh: string; labelEn: string }> = [
  { id: "global", labelZh: "国际服务", labelEn: "Global" },
  { id: "china", labelZh: "国内服务", labelEn: "China" },
  { id: "gateway", labelZh: "聚合网关", labelEn: "Gateways" },
  { id: "local", labelZh: "本地部署", labelEn: "Local" }
];

export const PROVIDERS_META = BUILT_IN_PROVIDERS.map((p) => ({
  id: p.id,
  name: p.name,
  nameEn: p.name,
  format: p.format,
  defaultBaseUrl: p.baseUrl,
  envKey: p.id.toUpperCase().replace(/-/g, "_") + "_API_KEY",
  defaultModels: p.models,
  category: "recommend" as const,
  gradient: p.gradient || "linear-gradient(135deg, #3B82F6 0%, #1D4ED8 100%)"
}));

function providerInitials(name: string) {
  return name.replace(/[^\p{L}\p{N}\s]/gu, "").trim().slice(0, 2).toUpperCase() || "AI";
}

function providerIconPath(id: string) {
  const aliases: Record<string, string> = {
    baidu: "baidu-qianfan",
    ali: "dashscope-coding",
    qwen: "qwen",
    google: "gemini"
  };
  const iconId = aliases[id] || id;
  return `/provider-icons/${iconId}.png`;
}

function ProviderMark({ provider }: { provider: Pick<ProviderConfigData, "id" | "name" | "gradient" | "icon"> }) {
  const [failed, setFailed] = useState(false);
  const src = provider.icon || providerIconPath(provider.id);
  return (
    <span className="provider-mark" style={{ background: provider.gradient }}>
      {!failed && (
        <img
          src={src}
          alt=""
          loading="lazy"
          onError={() => setFailed(true)}
        />
      )}
      <span>{providerInitials(provider.name)}</span>
    </span>
  );
}

function templateFor(id: string) {
  return BUILT_IN_PROVIDERS.find((p) => p.id === id);
}

function normalizeProvider(raw: unknown): ProviderConfigData {
  const provider = recordFromUnknown(raw) ?? {};
  const rawId = typeof provider.id === "string" ? provider.id : "";
  const rawName = typeof provider.name === "string" ? provider.name : "";
  const rawFormat = typeof provider.format === "string" ? provider.format : "";
  const preset = templateFor(rawId) || templateFor(rawName.toLowerCase());
  return {
    id: String(rawId || rawName || crypto.randomUUID()),
    name: String(rawName || preset?.name || rawId || "自定义提供商"),
    format: rawFormat === "anthropic" || rawFormat === "gemini" ? rawFormat : "openai",
    enabled: typeof provider.enabled === "boolean" ? provider.enabled : true,
    apiKey: typeof provider.apiKey === "string" ? provider.apiKey : "",
    baseUrl: typeof provider.baseUrl === "string" ? provider.baseUrl : preset?.baseUrl || "",
    models: Array.isArray(provider.models) ? provider.models.map(String) : preset?.models || [],
    gradient: typeof provider.gradient === "string" ? provider.gradient : preset?.gradient || "linear-gradient(135deg, #707782 0%, #4B525B 100%)",
    icon: typeof provider.icon === "string" ? provider.icon : preset?.icon
  };
}

function providerIdFromName(name: string) {
  const normalized = name.trim().toLowerCase();
  const known: Record<string, string> = {
    "豆包": "doubao",
    "火山": "doubao",
    "火山方舟": "doubao",
    "小米": "xiaomi",
    "通义千问": "qwen",
    "千问": "qwen",
    "智谱": "zhipu",
    "月之暗面": "moonshot",
    "kimi": "moonshot",
    "硅基流动": "siliconflow",
    "百度千帆": "baidu-qianfan",
    "阿里 coding": "dashscope-coding"
  };
  if (known[normalized]) {
    return known[normalized];
  }
  const ascii = normalized.replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return ascii || `provider-${crypto.randomUUID().slice(0, 8)}`;
}

export function ProvidersSettings({
  bootstrap,
  isZh,
  t
}: {
  bootstrap: Bootstrap;
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  const [searchQuery, setSearchQuery] = useState("");
  const [providers, setProviders] = useState<ProviderConfigData[]>(() => {
    if ("__TAURI_INTERNALS__" in window) {
      return [];
    }
    return loadStoredProviderValues().map(normalizeProvider);
  });

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [modalMode, setModalMode] = useState<"add" | "edit">("add");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formName, setFormName] = useState("");
  const [formFormat, setFormFormat] = useState<ProviderConfigData["format"]>("openai");
  const [formBaseUrl, setFormBaseUrl] = useState("");
  const [formApiKey, setFormApiKey] = useState("");
  const [formModels, setFormModels] = useState<string[]>([]);
  const [newModelInput, setNewModelInput] = useState("");
  const [visibleKey, setVisibleKey] = useState(false);
  const [checkState, setCheckState] = useState<"idle" | "checking" | "success" | "error">("idle");
  const [checkMessage, setCheckMessage] = useState("");
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [toastMessage, setToastMessage] = useState<string | null>(null);
  const [defaultLlm, setDefaultLlm] = useState<DefaultLlm>({
    provider: bootstrap.provider,
    model: bootstrap.model
  });

  useEffect(() => {
    if (deletingId) {
      const timer = setTimeout(() => {
        setDeletingId(null);
      }, 3000);
      return () => clearTimeout(timer);
    }
  }, [deletingId]);

  useEffect(() => {
    if (toastMessage) {
      const timer = setTimeout(() => {
        setToastMessage(null);
      }, 2000);
      return () => clearTimeout(timer);
    }
  }, [toastMessage]);

  useEffect(() => {
    if ("__TAURI_INTERNALS__" in window) {
      invoke<ProviderConfigData[]>("config_get_providers")
        .then((data) => {
          if (Array.isArray(data)) {
            const normalized = data.map(normalizeProvider);
            setProviders(normalized);
            saveStoredProviders(normalized);
          }
        })
        .catch(console.error);
    }
  }, []);

  useEffect(() => {
    if ("__TAURI_INTERNALS__" in window) {
      invoke<DefaultLlm>("config_get_default_llm")
        .then(setDefaultLlm)
        .catch(console.error);
    }
  }, []);

  const enabledCount = providers.filter((p) => p.enabled).length;
  const filteredProviders = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) {
      return providers;
    }
    return providers.filter((p) =>
      [p.name, p.id, p.baseUrl, p.format, ...p.models].some((value) =>
        value.toLowerCase().includes(query)
      )
    );
  }, [providers, searchQuery]);

  const saveProviders = (list: ProviderConfigData[]) => {
    setProviders(list);
    saveStoredProviders(list);
    if ("__TAURI_INTERNALS__" in window) {
      invoke("config_save_providers", { providers: list }).catch(console.error);
    }
  };

  const resetForm = () => {
    setFormName("");
    setFormFormat("openai");
    setFormBaseUrl("");
    setFormApiKey("");
    setFormModels([]);
    setNewModelInput("");
    setVisibleKey(false);
    setCheckState("idle");
    setCheckMessage("");
  };

  const applyTemplate = (preset: ProviderTemplate) => {
    setFormName(preset.name);
    setFormFormat(preset.format);
    setFormBaseUrl(preset.baseUrl);
    setFormModels([...preset.models]);
    setCheckState("idle");
    setCheckMessage("");
  };

  const openCustomModal = () => {
    resetForm();
    setModalMode("add");
    setEditingId(null);
    setIsModalOpen(true);
  };

  const openTemplateModal = (preset: ProviderTemplate) => {
    resetForm();
    setModalMode("add");
    setEditingId(null);
    applyTemplate(preset);
    setIsModalOpen(true);
  };

  const openEditModal = (provider: ProviderConfigData) => {
    setModalMode("edit");
    setEditingId(provider.id);
    setFormName(provider.name);
    setFormFormat(provider.format);
    setFormBaseUrl(provider.baseUrl);
    setFormApiKey(provider.apiKey);
    setFormModels([...provider.models]);
    setNewModelInput("");
    setVisibleKey(false);
    setCheckState("idle");
    setCheckMessage("");
    setIsModalOpen(true);
  };

  const buildFormProvider = (): ProviderConfigData | null => {
    const name = formName.trim();
    if (!name) {
      setCheckState("error");
      setCheckMessage(t("先填写提供商名称。", "Enter a provider name first."));
      return null;
    }
    const id = editingId || providerIdFromName(name);
    const existing = providers.find((p) => p.id === id);
    if (modalMode === "add" && existing) {
      setCheckState("error");
      setCheckMessage(t("这个提供商已经在列表里。", "This provider is already in the list."));
      return null;
    }
    const preset = templateFor(id);
    return {
      id,
      name,
      format: formFormat,
      enabled: modalMode === "edit" ? existing?.enabled ?? true : true,
      apiKey: formApiKey,
      baseUrl: formBaseUrl.trim(),
      models: formModels,
      gradient:
        existing?.gradient ||
        preset?.gradient ||
        "linear-gradient(135deg, #707782 0%, #4B525B 100%)"
    };
  };

  const handleSaveProvider = () => {
    const next = buildFormProvider();
    if (!next) {
      return;
    }
    saveProviders(
      modalMode === "add"
        ? [...providers, next]
        : providers.map((provider) => (provider.id === next.id ? next : provider))
    );
    setIsModalOpen(false);
  };

  const handleCheckProvider = async () => {
    const next = buildFormProvider();
    if (!next) {
      return;
    }
    if (!next.baseUrl.trim()) {
      setCheckState("error");
      setCheckMessage(t("先填写接口地址。", "Enter the API base URL first."));
      return;
    }
    if (!next.apiKey.trim() && next.id !== "ollama") {
      setCheckState("error");
      setCheckMessage(t("先填写 API Key，或改用本地 Ollama。", "Enter an API key, or use local Ollama."));
      return;
    }
    setCheckState("checking");
    setCheckMessage(t("正在读取可用模型。", "Fetching available models."));
    if ("__TAURI_INTERNALS__" in window) {
      try {
        await invoke("config_test_provider", { provider: next });
        setCheckState("success");
        setCheckMessage(t("配置可用。", "Configuration is reachable."));
      } catch (err) {
        setCheckState("error");
        setCheckMessage(String(err || t("检查失败。", "Check failed.")));
      }
    } else {
      window.setTimeout(() => {
        setCheckState("success");
        setCheckMessage(t("本地预览已通过基础校验。", "Local preview passed basic validation."));
      }, 450);
    }
  };

  const handleDeleteProvider = (provider: ProviderConfigData) => {
    if (deletingId === provider.id) {
      saveProviders(providers.filter((p) => p.id !== provider.id));
      setDeletingId(null);
    } else {
      setDeletingId(provider.id);
    }
  };

  const handleSetDefaultProvider = async (provider: ProviderConfigData, model?: string) => {
    const nextModel = model || provider.models[0] || defaultLlm.model;
    if (!provider.enabled) {
      setToastMessage(t("请先启用该提供商。", "Enable this provider first."));
      return;
    }
    if (!nextModel) {
      setToastMessage(t("请先添加一个模型。", "Add a model first."));
      return;
    }
    const nextDefault = { provider: provider.id, model: nextModel };
    setDefaultLlm(nextDefault);
    saveLastModelForProvider(provider.id, nextModel);
    if ("__TAURI_INTERNALS__" in window) {
      try {
        const saved = await invoke<DefaultLlm>("config_set_default_llm", nextDefault);
        setDefaultLlm(saved);
        dispatchDefaultLlmChange(saved);
      } catch (err) {
        console.error(err);
        setToastMessage(String(err || t("设置默认模型失败。", "Failed to set default model.")));
        return;
      }
    } else {
      dispatchDefaultLlmChange(nextDefault);
    }
    setToastMessage(t("已设为新对话默认模型。", "Default model for new chats updated."));
  };

  const handleAddModelTag = () => {
    const next = newModelInput.trim();
    if (next && !formModels.includes(next)) {
      setFormModels([...formModels, next]);
    }
    setNewModelInput("");
  };

  const availableTemplates = BUILT_IN_PROVIDERS.filter((preset) => !providers.some((p) => p.id === preset.id));
  const defaultModelLabel =
    defaultLlm.provider && defaultLlm.model
      ? `${defaultLlm.provider} / ${defaultLlm.model}`
      : defaultLlm.model || defaultLlm.provider || t("未配置", "Not configured");

  return (
    <div className="providers-page">
      <div className="providers-toolbar">
        <div>
          <p className="providers-kicker">{t("模型提供商", "Model providers")}</p>
          <p className="providers-summary">
            {t(
              `${providers.length} 个配置，${enabledCount} 个启用。默认：${defaultModelLabel}`,
              `${providers.length} configured, ${enabledCount} enabled. Default: ${defaultModelLabel}`
            )}
          </p>
        </div>
        <div className="provider-add-menu">
          <button onClick={openCustomModal} type="button" className="primary-button providers-add-button">
            <Plus size={14} />
            <span>{t("添加", "Add")}</span>
            <ChevronDown size={13} />
          </button>
          <div className="provider-add-dropdown">
            <button type="button" className="provider-add-option custom" onClick={openCustomModal}>
              <span className="provider-mark custom">
                <Plus size={14} />
              </span>
              <span className="provider-add-option-body">
                <span className="provider-add-option-title">
                  <strong>{t("自定义接口", "Custom endpoint")}</strong>
                  <em>Custom</em>
                </span>
                <small>{t("填写兼容 OpenAI、Anthropic 或 Gemini 的接口", "Use any compatible endpoint")}</small>
              </span>
            </button>
            {availableTemplates.length > 0 && (
              <>
                {PROVIDER_GROUPS.map((group) => {
                  const groupTemplates = availableTemplates.filter((preset) => preset.group === group.id);
                  if (groupTemplates.length === 0) {
                    return null;
                  }
                  return (
                    <div key={group.id} className="provider-add-section">
                      <div className="provider-add-dropdown-label">
                        {t(group.labelZh, group.labelEn)}
                      </div>
                      {groupTemplates.map((preset) => (
                        <button key={preset.id} type="button" className="provider-add-option" onClick={() => openTemplateModal(preset)}>
                          <ProviderMark provider={preset} />
                          <span className="provider-add-option-body">
                            <span className="provider-add-option-title">
                              <strong>{preset.name}</strong>
                              <em>{preset.format === "openai" ? "OpenAI" : preset.format}</em>
                            </span>
                            <small>
                              {preset.note}
                              {preset.models[0] ? ` · ${preset.models[0]}` : ""}
                            </small>
                          </span>
                        </button>
                      ))}
                    </div>
                  );
                })}
              </>
            )}
          </div>
        </div>
      </div>

      <label className="providers-search">
        <Search size={14} />
        <input
          type="text"
          placeholder={t("搜索名称、模型或接口地址", "Search name, model, or base URL")}
          value={searchQuery}
          onChange={(event) => setSearchQuery(event.target.value)}
        />
      </label>

      <div className="provider-list">
        {filteredProviders.length === 0 ? (
          <div className="provider-empty">
            {t("没有匹配的提供商。", "No matching providers.")}
          </div>
        ) : (
          filteredProviders.map((provider) => (
            <div key={provider.id} className="provider-row">
              <ProviderMark provider={provider} />
              <div className="provider-main">
                <div className="provider-title-line">
                  <strong>{provider.name}</strong>
                  <span className={provider.enabled ? "provider-state active" : "provider-state"}>
                    {provider.enabled ? t("启用", "Enabled") : t("停用", "Disabled")}
                  </span>
                </div>
                <div className="provider-meta">
                  <span>{provider.format === "openai" ? "OpenAI compatible" : provider.format}</span>
                  <span>{provider.baseUrl || t("未填写接口地址", "No base URL")}</span>
                </div>
                {provider.models.length > 0 && (
                  <div className="provider-models">
                    {provider.models.slice(0, 4).map((model) => (
                      <code
                        key={model}
                        className={defaultLlm.provider === provider.id && defaultLlm.model === model ? "default" : ""}
                        onClick={() => void handleSetDefaultProvider(provider, model)}
                        title={t("点击设为新对话默认模型", "Click to set as default for new chats")}
                      >
                        {model}
                      </code>
                    ))}
                    {provider.models.length > 4 && <span>+{provider.models.length - 4}</span>}
                  </div>
                )}
              </div>
              <div className="provider-actions">
                <button
                  type="button"
                  className={defaultLlm.provider === provider.id ? "provider-default-button active" : "provider-default-button"}
                  onClick={() => void handleSetDefaultProvider(provider)}
                  title={t("设为新对话默认", "Set as default for new chats")}
                >
                  {defaultLlm.provider === provider.id ? t("默认", "Default") : t("设默认", "Default")}
                </button>
                <button
                  type="button"
                  className={provider.enabled ? "provider-switch active" : "provider-switch"}
                  aria-label={provider.enabled ? t("停用", "Disable") : t("启用", "Enable")}
                  onClick={() =>
                    saveProviders(
                      providers.map((item) =>
                        item.id === provider.id ? { ...item, enabled: !item.enabled } : item
                      )
                    )
                  }
                >
                  <span />
                </button>
                <button type="button" className="provider-icon-button" title={t("编辑", "Edit")} onClick={() => openEditModal(provider)}>
                  <Edit2 size={15} />
                </button>
                <button
                  type="button"
                  className={`provider-icon-button danger ${deletingId === provider.id ? "confirming" : ""}`}
                  title={deletingId === provider.id ? t("再次点击确认删除", "Click again to confirm delete") : t("删除", "Delete")}
                  onClick={() => handleDeleteProvider(provider)}
                >
                  {deletingId === provider.id ? <Check size={15} /> : <Trash2 size={15} />}
                </button>
              </div>
            </div>
          ))
        )}
      </div>

      {isModalOpen && (
        <div className="provider-modal-backdrop">
          <div className="provider-modal">
            <div className="provider-modal-header">
              <div>
                <h2>{modalMode === "add" ? t("添加提供商", "Add provider") : t("编辑提供商", "Edit provider")}</h2>
                <p>{t("保存后会写入本机配置。", "Saved locally to your configuration.")}</p>
              </div>
              <button type="button" className="provider-icon-button" onClick={() => setIsModalOpen(false)}>
                <X size={15} />
              </button>
            </div>

            <div className="provider-form">
              <div className="form-group">
                <span><Bot size={12} />{t("名称", "Name")}</span>
                <input value={formName} onChange={(event) => setFormName(event.target.value)} disabled={modalMode === "edit"} placeholder="Moonshot AI" />
              </div>
              <div className="form-group">
                <span><Globe size={12} />{t("接口格式", "Format")}</span>
                <CustomSelect
                  value={formFormat}
                  onChange={(value) => {
                    if (isProviderFormat(value)) setFormFormat(value);
                  }}
                  options={[
                    { value: "openai", label: "OpenAI compatible", avatarText: "OP" },
                    { value: "anthropic", label: "Anthropic", avatarText: "AN" },
                    { value: "gemini", label: "Gemini", avatarText: "GE" }
                  ]}
                  style={{ width: "100%" }}
                />
              </div>
              <div className="form-group">
                <span><Globe size={12} />{t("接口地址", "Base URL")}</span>
                <input value={formBaseUrl} onChange={(event) => setFormBaseUrl(event.target.value)} placeholder="https://api.example.com/v1" />
              </div>
              <div className="form-group">
                <span><KeyRound size={12} />API Key</span>
                <div className="provider-secret-field">
                  <input
                    type={visibleKey ? "text" : "password"}
                    value={formApiKey}
                    onChange={(event) => setFormApiKey(event.target.value)}
                    placeholder={t("留空则使用环境变量", "Leave blank to use environment variables")}
                  />
                  <button type="button" onClick={() => setVisibleKey(!visibleKey)}>
                    {visibleKey ? <EyeOff size={14} /> : <Eye size={14} />}
                  </button>
                </div>
              </div>
              <div className="form-group">
                <span><Bot size={12} />{t("模型", "Models")}</span>
                {formModels.length > 0 && (
                  <div className="provider-model-editor">
                    {formModels.map((model) => (
                      <code
                        key={model}
                        onClick={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          navigator.clipboard.writeText(model);
                          setToastMessage(t(`已复制模型名称 “${model}”`, `Model name "${model}" copied`));
                        }}
                        title={t("点击复制到剪贴板", "Click to copy to clipboard")}
                        style={{ cursor: "pointer" }}
                      >
                        {model}
                        <button
                          type="button"
                          onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            setFormModels(formModels.filter((item) => item !== model));
                          }}
                        >
                          <X size={10} />
                        </button>
                      </code>
                    ))}
                  </div>
                )}
                <div className="provider-model-add">
                  <input
                    value={newModelInput}
                    onChange={(event) => setNewModelInput(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        handleAddModelTag();
                      }
                    }}
                    placeholder="gpt-4.1"
                    autoComplete="off"
                    autoCorrect="off"
                    autoCapitalize="off"
                    spellCheck={false}
                  />
                  <button type="button" className="secondary-button" onClick={handleAddModelTag}>
                    <Plus size={13} />
                  </button>
                </div>
              </div>
            </div>

            {checkMessage && (
              <div className={`provider-check-message ${checkState}`}>
                {checkState === "success" ? <Check size={14} /> : checkState === "error" ? <AlertCircle size={14} /> : <ShieldCheck size={14} />}
                <span>{checkMessage}</span>
              </div>
            )}

            <div className="provider-modal-footer">
              <button type="button" className="secondary-button" onClick={handleCheckProvider} disabled={checkState === "checking"}>
                <ShieldCheck size={14} />
                <span>{checkState === "checking" ? t("检查中", "Checking") : t("检查配置", "Check")}</span>
              </button>
              <div>
                <button type="button" className="ghost-button" onClick={() => setIsModalOpen(false)}>
                  {t("取消", "Cancel")}
                </button>
                <button type="button" className="primary-button" onClick={handleSaveProvider}>
                  {t("保存", "Save")}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
      {toastMessage && (
        <div className="provider-toast">
          {toastMessage}
        </div>
      )}
    </div>
  );
}
