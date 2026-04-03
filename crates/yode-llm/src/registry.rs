use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::provider::LlmProvider;

/// Known provider info for auto-detection.
pub struct ProviderInfo {
    pub name: &'static str,
    pub format: &'static str,
    pub env_keys: &'static [&'static str],
    pub default_base_url: &'static str,
    pub default_models: &'static [&'static str],
}

/// Built-in provider catalog.
pub const KNOWN_PROVIDERS: &[ProviderInfo] = &[
    ProviderInfo {
        name: "anthropic",
        format: "anthropic",
        env_keys: &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"],
        default_base_url: "https://api.anthropic.com",
        default_models: &[
            "claude-sonnet-4-20250514",
            "claude-opus-4-20250514",
            "claude-haiku-4-20250414",
        ],
    },
    ProviderInfo {
        name: "openai",
        format: "openai",
        env_keys: &["OPENAI_API_KEY"],
        default_base_url: "https://api.openai.com/v1",
        default_models: &["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "o1", "o1-mini", "o3-mini"],
    },
    ProviderInfo {
        name: "google",
        format: "gemini",
        env_keys: &["GOOGLE_API_KEY", "GEMINI_API_KEY"],
        default_base_url: "https://generativelanguage.googleapis.com/v1beta",
        default_models: &["gemini-2.5-pro", "gemini-2.5-flash", "gemini-2.0-flash"],
    },
    ProviderInfo {
        name: "deepseek",
        format: "openai",
        env_keys: &["DEEPSEEK_API_KEY"],
        default_base_url: "https://api.deepseek.com/v1",
        default_models: &["deepseek-chat", "deepseek-reasoner"],
    },
    ProviderInfo {
        name: "groq",
        format: "openai",
        env_keys: &["GROQ_API_KEY"],
        default_base_url: "https://api.groq.com/openai/v1",
        default_models: &["llama-3.3-70b-versatile", "llama-3.1-8b-instant", "mixtral-8x7b-32768"],
    },
    ProviderInfo {
        name: "mistral",
        format: "openai",
        env_keys: &["MISTRAL_API_KEY"],
        default_base_url: "https://api.mistral.ai/v1",
        default_models: &["mistral-large-latest", "mistral-medium-latest", "codestral-latest"],
    },
    ProviderInfo {
        name: "xai",
        format: "openai",
        env_keys: &["XAI_API_KEY"],
        default_base_url: "https://api.x.ai/v1",
        default_models: &["grok-3", "grok-3-mini", "grok-2"],
    },
    ProviderInfo {
        name: "openrouter",
        format: "openai",
        env_keys: &["OPENROUTER_API_KEY"],
        default_base_url: "https://openrouter.ai/api/v1",
        default_models: &[],
    },
    ProviderInfo {
        name: "ollama",
        format: "openai",
        env_keys: &[],
        default_base_url: "http://localhost:11434/v1",
        default_models: &["llama3.1", "qwen2.5-coder", "deepseek-coder-v2"],
    },
    ProviderInfo {
        name: "together",
        format: "openai",
        env_keys: &["TOGETHER_API_KEY"],
        default_base_url: "https://api.together.xyz/v1",
        default_models: &[],
    },
    ProviderInfo {
        name: "fireworks",
        format: "openai",
        env_keys: &["FIREWORKS_API_KEY"],
        default_base_url: "https://api.fireworks.ai/inference/v1",
        default_models: &[],
    },
    ProviderInfo {
        name: "perplexity",
        format: "openai",
        env_keys: &["PERPLEXITY_API_KEY"],
        default_base_url: "https://api.perplexity.ai",
        default_models: &["sonar-pro", "sonar"],
    },
    ProviderInfo {
        name: "cerebras",
        format: "openai",
        env_keys: &["CEREBRAS_API_KEY"],
        default_base_url: "https://api.cerebras.ai/v1",
        default_models: &["llama-3.3-70b"],
    },
    ProviderInfo {
        name: "azure",
        format: "openai",
        env_keys: &["AZURE_OPENAI_API_KEY"],
        default_base_url: "",
        default_models: &[],
    },
    // ── 国内提供商 ──
    ProviderInfo {
        name: "qwen",
        format: "openai",
        env_keys: &["DASHSCOPE_API_KEY", "QWEN_API_KEY"],
        default_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        default_models: &["qwen3.5-plus", "qwen-max", "qwen-plus", "qwen-turbo", "qwen3-coder-30b-a3b-instruct"],
    },
    ProviderInfo {
        name: "alibaba-coding",
        format: "openai",
        env_keys: &["ALIBABA_CODING_PLAN_API_KEY"],
        default_base_url: "https://coding.dashscope.aliyuncs.com/v1",
        default_models: &["qwen3.5-plus", "qwen3-coder-next", "kimi-k2.5", "MiniMax-M2.5", "glm-5"],
    },
    ProviderInfo {
        name: "zhipu",
        format: "openai",
        env_keys: &["ZHIPU_API_KEY", "GLM_API_KEY"],
        default_base_url: "https://open.bigmodel.cn/api/paas/v4",
        default_models: &["glm-5", "glm-4.7", "glm-4.5v", "glm-4.5-air"],
    },
    ProviderInfo {
        name: "moonshot",
        format: "openai",
        env_keys: &["MOONSHOT_API_KEY", "KIMI_API_KEY"],
        default_base_url: "https://api.moonshot.cn/v1",
        default_models: &["kimi-k2.5", "kimi-k2-thinking", "kimi-k2-thinking-turbo"],
    },
    ProviderInfo {
        name: "kimi-coding",
        format: "anthropic",
        env_keys: &["KIMI_API_KEY"],
        default_base_url: "https://api.kimi.com/coding/v1",
        default_models: &["kimi-k2-thinking", "k2p5"],
    },
    ProviderInfo {
        name: "doubao",
        format: "openai",
        env_keys: &["ARK_API_KEY", "DOUBAO_API_KEY"],
        default_base_url: "https://ark.cn-beijing.volces.com/api/v3",
        default_models: &["doubao-pro-256k", "doubao-pro-32k", "doubao-lite-32k"],
    },
    ProviderInfo {
        name: "minimax",
        format: "anthropic",
        env_keys: &["MINIMAX_API_KEY"],
        default_base_url: "https://api.minimaxi.com/anthropic/v1",
        default_models: &["MiniMax-M2.7", "MiniMax-M2.5", "MiniMax-M2.1", "MiniMax-M2"],
    },
    ProviderInfo {
        name: "siliconflow",
        format: "openai",
        env_keys: &["SILICONFLOW_API_KEY", "SF_API_KEY"],
        default_base_url: "https://api.siliconflow.cn/v1",
        default_models: &["deepseek-ai/DeepSeek-V3", "Qwen/Qwen2.5-72B-Instruct", "THUDM/GLM-4-32B-0414"],
    },
    ProviderInfo {
        name: "yi",
        format: "openai",
        env_keys: &["YI_API_KEY", "LINGYIWANWU_API_KEY"],
        default_base_url: "https://api.lingyiwanwu.com/v1",
        default_models: &["yi-lightning", "yi-large", "yi-medium"],
    },
    ProviderInfo {
        name: "baichuan",
        format: "openai",
        env_keys: &["BAICHUAN_API_KEY"],
        default_base_url: "https://api.baichuan-ai.com/v1",
        default_models: &["Baichuan4", "Baichuan3-Turbo", "Baichuan3-Turbo-128k"],
    },
    ProviderInfo {
        name: "spark",
        format: "openai",
        env_keys: &["SPARK_API_KEY", "IFLYTEK_API_KEY"],
        default_base_url: "https://spark-api-open.xf-yun.com/v1",
        default_models: &["generalv3.5", "4.0Ultra"],
    },
    ProviderInfo {
        name: "stepfun",
        format: "openai",
        env_keys: &["STEPFUN_API_KEY"],
        default_base_url: "https://api.stepfun.com/v1",
        default_models: &["step-3.5-flash", "step-2-16k", "step-1-32k"],
    },
    ProviderInfo {
        name: "ernie",
        format: "openai",
        env_keys: &["QIANFAN_API_KEY", "ERNIE_API_KEY"],
        default_base_url: "https://qianfan.baidubce.com/v2",
        default_models: &["ernie-4.0-8k", "ernie-3.5-8k", "ernie-speed-128k"],
    },
    ProviderInfo {
        name: "hunyuan",
        format: "openai",
        env_keys: &["HUNYUAN_API_KEY"],
        default_base_url: "https://api.hunyuan.cloud.tencent.com/v1",
        default_models: &["hunyuan-pro", "hunyuan-standard", "hunyuan-lite"],
    },
    ProviderInfo {
        name: "tencent-coding",
        format: "openai",
        env_keys: &["TENCENT_CODING_PLAN_API_KEY"],
        default_base_url: "https://api.lkeap.cloud.tencent.com/coding/v3",
        default_models: &["hunyuan-2.0-instruct", "kimi-k2.5", "hunyuan-t1", "tc-code-latest"],
    },
    ProviderInfo {
        name: "bailing",
        format: "openai",
        env_keys: &["BAILING_API_TOKEN"],
        default_base_url: "https://api.tbox.cn/api/llm/v1/chat/completions",
        default_models: &["Ling-1T", "Ring-1T"],
    },
    ProviderInfo {
        name: "iflow",
        format: "openai",
        env_keys: &["IFLOW_API_KEY"],
        default_base_url: "https://apis.iflow.cn/v1",
        default_models: &["deepseek-r1", "deepseek-v3", "kimi-k2", "qwen3-max-preview"],
    },
];

/// Look up a known provider by name.
pub fn find_provider_info(name: &str) -> Option<&'static ProviderInfo> {
    KNOWN_PROVIDERS.iter().find(|p| p.name == name)
}

/// Detect available providers from environment variables.
pub fn detect_available_providers() -> Vec<&'static ProviderInfo> {
    KNOWN_PROVIDERS
        .iter()
        .filter(|p| {
            // Ollama is always "available" (local)
            if p.name == "ollama" {
                return true;
            }
            p.env_keys.iter().any(|key| std::env::var(key).is_ok())
        })
        .collect()
}

pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, Arc<dyn LlmProvider>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, provider: Arc<dyn LlmProvider>) {
        let name = provider.name().to_string();
        self.providers.write().unwrap().insert(name, provider);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn LlmProvider>> {
        self.providers.read().unwrap().get(name).cloned()
    }

    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = self.providers.read().unwrap().keys().cloned().collect();
        names.sort();
        names
    }

    pub fn remove(&self, name: &str) -> Option<Arc<dyn LlmProvider>> {
        self.providers.write().unwrap().remove(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.providers.read().unwrap().contains_key(name)
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
