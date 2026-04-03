//! OpenAI-compatible providers.
//!
//! Many LLM providers expose an OpenAI-compatible chat/completions API.
//! This module provides thin wrappers that set the correct base_url,
//! auth headers, and provider name while reusing OpenAiProvider.

use super::openai::OpenAiProvider;

// ── 海外提供商 (International) ──────────────────────────────────────────────

/// Ollama (本地模型, 无需 API Key)
pub fn ollama(base_url: Option<&str>) -> OpenAiProvider {
    let url = base_url.unwrap_or("http://localhost:11434/v1");
    OpenAiProvider::new("ollama", "", url)
}

/// Groq (超快推理)
pub fn groq(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("groq", api_key, "https://api.groq.com/openai/v1")
}

/// Mistral AI
pub fn mistral(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("mistral", api_key, "https://api.mistral.ai/v1")
}

/// DeepSeek
pub fn deepseek(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("deepseek", api_key, "https://api.deepseek.com")
}

/// xAI (Grok)
pub fn xai(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("xai", api_key, "https://api.x.ai/v1")
}

/// OpenRouter (统一网关)
pub fn openrouter(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("openrouter", api_key, "https://openrouter.ai/api/v1")
}

/// Azure OpenAI
pub fn azure(api_key: &str, endpoint: &str) -> OpenAiProvider {
    OpenAiProvider::new("azure", api_key, endpoint)
}

/// Together AI
pub fn together(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("together", api_key, "https://api.together.xyz/v1")
}

/// Fireworks AI
pub fn fireworks(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("fireworks", api_key, "https://api.fireworks.ai/inference/v1")
}

/// Perplexity
pub fn perplexity(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("perplexity", api_key, "https://api.perplexity.ai")
}

/// Cerebras
pub fn cerebras(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("cerebras", api_key, "https://api.cerebras.ai/v1")
}

// ── 国内提供商 (Chinese Providers) ──────────────────────────────────────────

/// 通义千问 Qwen - 阿里云百炼 (国内)
pub fn qwen(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("qwen", api_key, "https://dashscope.aliyuncs.com/compatible-mode/v1")
}

/// 通义千问 Qwen - 阿里云百炼 (国际)
pub fn qwen_intl(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("qwen-intl", api_key, "https://dashscope-intl.aliyuncs.com/compatible-mode/v1")
}

/// 阿里 Coding Plan (国内, 支持 kimi/qwen/minimax/glm 多模型)
pub fn alibaba_coding(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("alibaba-coding", api_key, "https://coding.dashscope.aliyuncs.com/v1")
}

/// 智谱 GLM (Zhipu AI)
pub fn zhipu(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("zhipu", api_key, "https://open.bigmodel.cn/api/paas/v4")
}

/// 智谱 Coding Plan
pub fn zhipu_coding(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("zhipu-coding", api_key, "https://open.bigmodel.cn/api/coding/paas/v4")
}

/// Kimi / Moonshot (月之暗面, 国内端点)
pub fn moonshot(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("moonshot", api_key, "https://api.moonshot.cn/v1")
}

/// Kimi / Moonshot (国际端点)
pub fn moonshot_intl(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("moonshot-intl", api_key, "https://api.moonshot.ai/v1")
}

/// Kimi for Coding (月之暗面编程专用, Anthropic 兼容格式)
/// 注意: 此端点使用 Anthropic 消息格式, 需要 AnthropicProvider
pub fn kimi_coding_url() -> &'static str {
    "https://api.kimi.com/coding/v1"
}

/// 豆包 Doubao (字节跳动/火山引擎)
pub fn doubao(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("doubao", api_key, "https://ark.cn-beijing.volces.com/api/v3")
}

/// MiniMax (国际端点, Anthropic 兼容)
/// 注意: MiniMax 新 API 使用 Anthropic 格式
pub fn minimax_url() -> &'static str {
    "https://api.minimax.io/anthropic/v1"
}

/// MiniMax (国内端点, Anthropic 兼容)
pub fn minimax_cn_url() -> &'static str {
    "https://api.minimaxi.com/anthropic/v1"
}

/// 硅基流动 SiliconFlow (国际)
pub fn siliconflow(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("siliconflow", api_key, "https://api.siliconflow.com/v1")
}

/// 硅基流动 SiliconFlow (国内)
pub fn siliconflow_cn(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("siliconflow-cn", api_key, "https://api.siliconflow.cn/v1")
}

/// 零一万物 Yi (01.AI)
pub fn yi(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("yi", api_key, "https://api.lingyiwanwu.com/v1")
}

/// 百川 Baichuan (百川智能)
pub fn baichuan(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("baichuan", api_key, "https://api.baichuan-ai.com/v1")
}

/// 讯飞星火 Spark (科大讯飞)
pub fn spark(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("spark", api_key, "https://spark-api-open.xf-yun.com/v1")
}

/// 阶跃星辰 StepFun
pub fn stepfun(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("stepfun", api_key, "https://api.stepfun.com/v1")
}

/// 文心一言 ERNIE (百度千帆)
pub fn ernie(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("ernie", api_key, "https://qianfan.baidubce.com/v2")
}

/// 腾讯混元 Hunyuan
pub fn hunyuan(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("hunyuan", api_key, "https://api.hunyuan.cloud.tencent.com/v1")
}

/// 腾讯 Coding Plan
pub fn tencent_coding(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("tencent-coding", api_key, "https://api.lkeap.cloud.tencent.com/coding/v3")
}

/// 百灵 Bailing
pub fn bailing(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("bailing", api_key, "https://api.tbox.cn/api/llm/v1/chat/completions")
}

/// iFlow (国内聚合平台)
pub fn iflow(api_key: &str) -> OpenAiProvider {
    OpenAiProvider::new("iflow", api_key, "https://apis.iflow.cn/v1")
}

// ── 通用 ────────────────────────────────────────────────────────────────────

/// 自定义 OpenAI 兼容提供商
pub fn custom(name: &str, api_key: &str, base_url: &str) -> OpenAiProvider {
    OpenAiProvider::new(name, api_key, base_url)
}
