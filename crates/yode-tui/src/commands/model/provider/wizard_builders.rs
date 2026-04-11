use crate::app::wizard::{Wizard, WizardStep};

use super::config_ops::add_provider_to_config;

pub(super) fn build_add_provider_wizard() -> Wizard {
    Wizard::new(
        "添加 LLM 提供商".into(),
        vec![
            WizardStep::Select {
                prompt: "选择提供商类型:".into(),
                options: vec![
                    "Anthropic (Claude)".into(),
                    "OpenAI (GPT)".into(),
                    "Google Gemini".into(),
                    "DeepSeek".into(),
                    "通义千问 Qwen (阿里)".into(),
                    "智谱 GLM (Zhipu)".into(),
                    "Kimi (月之暗面)".into(),
                    "豆包 Doubao (字节)".into(),
                    "硅基流动 SiliconFlow".into(),
                    "Groq".into(),
                    "Mistral".into(),
                    "xAI (Grok)".into(),
                    "Ollama (本地)".into(),
                    "OpenRouter".into(),
                    "零一万物 Yi".into(),
                    "百川 Baichuan".into(),
                    "讯飞星火 Spark".into(),
                    "MiniMax".into(),
                    "阶跃星辰 StepFun".into(),
                    "文心一言 ERNIE (百度)".into(),
                    "腾讯混元 Hunyuan".into(),
                    "阿里 Coding Plan (聚合)".into(),
                    "腾讯 Coding Plan (聚合)".into(),
                    "百灵 Bailing".into(),
                    "iFlow (聚合)".into(),
                    "自定义 (Custom)".into(),
                ],
                default: 0,
                key: "provider_type".into(),
            },
            WizardStep::Input {
                prompt: "Base URL:".into(),
                default: Some("https://api.anthropic.com".into()),
                key: "base_url".into(),
            },
            WizardStep::Input {
                prompt: "API Key:".into(),
                default: None,
                key: "api_key".into(),
            },
            WizardStep::Input {
                prompt: "Provider 名称:".into(),
                default: Some("anthropic".into()),
                key: "name".into(),
            },
            WizardStep::Input {
                prompt: "默认模型:".into(),
                default: Some("claude-sonnet-4-20250514".into()),
                key: "model".into(),
            },
        ],
        Box::new(|answers| {
            let provider_type = answers.get("provider_type").ok_or("Missing type")?;
            let name = answers.get("name").ok_or("Missing name")?;
            let base_url = answers.get("base_url").ok_or("Missing base_url")?;
            let api_key = answers.get("api_key").ok_or("Missing api_key")?;
            let model = answers.get("model").cloned().unwrap_or_default();

            let format = if provider_type.contains("Anthropic") {
                "anthropic"
            } else if provider_type.contains("Gemini") {
                "gemini"
            } else {
                "openai"
            };

            let models = if model.is_empty() {
                vec![]
            } else {
                model
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            };

            add_provider_to_config(
                name,
                format,
                Some(base_url.as_str()),
                &models,
                Some(api_key),
            )?;

            Ok(vec![
                format!("✓ Provider '{}' 已添加!", name),
                format!("  format:   {}", format),
                format!("  base_url: {}", base_url),
                format!(
                    "  model:    {}",
                    if model.is_empty() {
                        "(unrestricted)"
                    } else {
                        &model
                    }
                ),
                format!(
                    "  api_key:  {}...{}",
                    &api_key[..4.min(api_key.len())],
                    &api_key[api_key.len().saturating_sub(4)..]
                ),
                String::new(),
                "重启 yode 以激活新提供商。".into(),
            ])
        }),
    )
    .with_step_callback(Box::new(|value, steps| {
        let (default_url, name_hint, model_hint) = match value {
            v if v.contains("Anthropic") => (
                "https://api.anthropic.com",
                "anthropic",
                "claude-sonnet-4-20250514",
            ),
            v if v.contains("OpenAI") => ("https://api.openai.com/v1", "openai", "gpt-4o"),
            v if v.contains("Gemini") => (
                "https://generativelanguage.googleapis.com/v1beta",
                "google",
                "gemini-2.5-flash",
            ),
            v if v.contains("DeepSeek") => {
                ("https://api.deepseek.com/v1", "deepseek", "deepseek-chat")
            }
            v if v.contains("Qwen") || v.contains("千问") => (
                "https://dashscope.aliyuncs.com/compatible-mode/v1",
                "qwen",
                "qwen-max",
            ),
            v if v.contains("Zhipu") || v.contains("智谱") => (
                "https://open.bigmodel.cn/api/paas/v4",
                "zhipu",
                "glm-4-plus",
            ),
            v if v.contains("Kimi") || v.contains("月之暗面") => {
                ("https://api.moonshot.cn/v1", "moonshot", "moonshot-v1-auto")
            }
            v if v.contains("Doubao") || v.contains("豆包") => (
                "https://ark.cn-beijing.volces.com/api/v3",
                "doubao",
                "doubao-pro-256k",
            ),
            v if v.contains("SiliconFlow") || v.contains("硅基") => (
                "https://api.siliconflow.cn/v1",
                "siliconflow",
                "deepseek-ai/DeepSeek-V3",
            ),
            v if v.contains("Yi") || v.contains("零一") => {
                ("https://api.lingyiwanwu.com/v1", "yi", "yi-lightning")
            }
            v if v.contains("Baichuan") || v.contains("百川") => {
                ("https://api.baichuan-ai.com/v1", "baichuan", "Baichuan4")
            }
            v if v.contains("Spark") || v.contains("星火") => (
                "https://spark-api-open.xf-yun.com/v1",
                "spark",
                "generalv3.5",
            ),
            v if v.contains("MiniMax") => {
                ("https://api.minimax.chat/v1", "minimax", "MiniMax-Text-01")
            }
            v if v.contains("StepFun") || v.contains("阶跃") => {
                ("https://api.stepfun.com/v1", "stepfun", "step-2-16k")
            }
            v if v.contains("ERNIE") || v.contains("文心") => {
                ("https://qianfan.baidubce.com/v2", "ernie", "ernie-4.0-8k")
            }
            v if v.contains("Hunyuan") || v.contains("混元") => (
                "https://api.hunyuan.cloud.tencent.com/v1",
                "hunyuan",
                "hunyuan-pro",
            ),
            v if v.contains("阿里 Coding") => (
                "https://coding.dashscope.aliyuncs.com/v1",
                "alibaba-coding",
                "qwen3.5-plus",
            ),
            v if v.contains("腾讯 Coding") => (
                "https://api.lkeap.cloud.tencent.com/coding/v3",
                "tencent-coding",
                "hunyuan-2.0-instruct",
            ),
            v if v.contains("Bailing") || v.contains("百灵") => (
                "https://api.tbox.cn/api/llm/v1/chat/completions",
                "bailing",
                "Ling-1T",
            ),
            v if v.contains("iFlow") => ("https://apis.iflow.cn/v1", "iflow", "deepseek-r1"),
            v if v.contains("Groq") => (
                "https://api.groq.com/openai/v1",
                "groq",
                "llama-3.3-70b-versatile",
            ),
            v if v.contains("Mistral") => (
                "https://api.mistral.ai/v1",
                "mistral",
                "mistral-large-latest",
            ),
            v if v.contains("xAI") => ("https://api.x.ai/v1", "xai", "grok-3"),
            v if v.contains("Ollama") => ("http://localhost:11434/v1", "ollama", "llama3.1"),
            v if v.contains("OpenRouter") => (
                "https://openrouter.ai/api/v1",
                "openrouter",
                "anthropic/claude-sonnet-4",
            ),
            _ => return,
        };
        if let Some(WizardStep::Input { default, .. }) = steps.get_mut(1) {
            *default = Some(default_url.into());
        }
        if let Some(WizardStep::Input { default, .. }) = steps.get_mut(3) {
            *default = Some(name_hint.into());
        }
        if let Some(WizardStep::Input { default, .. }) = steps.get_mut(4) {
            *default = Some(model_hint.into());
        }
    }))
}

pub(super) fn build_edit_provider_wizard(
    name: &str,
) -> Result<crate::app::wizard::Wizard, String> {
    let config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    let p = config
        .llm
        .providers
        .get(name)
        .ok_or_else(|| format!("Provider '{}' not found in config.", name))?;

    let current_format = p.format.clone();
    let current_url = p.base_url.clone().unwrap_or_default();
    let current_api_key = p.api_key.clone().unwrap_or_default();
    let current_models = if p.models.is_empty() {
        String::new()
    } else {
        p.models.join(", ")
    };
    let provider_name = name.to_string();

    let format_default = match current_format.as_str() {
        "anthropic" => 1,
        "gemini" => 2,
        _ => 0,
    };

    let masked_key = if current_api_key.len() > 8 {
        format!(
            "{}...{}",
            &current_api_key[..4],
            &current_api_key[current_api_key.len() - 4..]
        )
    } else if !current_api_key.is_empty() {
        "****".to_string()
    } else {
        String::new()
    };

    Ok(Wizard::new(
        format!("Editing provider '{}' (Enter to keep current)", name),
        vec![
            WizardStep::Select {
                prompt: "API format:".into(),
                options: vec!["openai".into(), "anthropic".into(), "gemini".into()],
                default: format_default,
                key: "format".into(),
            },
            WizardStep::Input {
                prompt: "Base URL:".into(),
                default: Some(current_url),
                key: "base_url".into(),
            },
            WizardStep::Input {
                prompt: format!(
                    "API Key (current: {}): ",
                    if masked_key.is_empty() {
                        "not set"
                    } else {
                        &masked_key
                    }
                ),
                default: Some(current_api_key),
                key: "api_key".into(),
            },
            WizardStep::Input {
                prompt: "Models (comma-separated, empty for unrestricted):".into(),
                default: Some(current_models),
                key: "models".into(),
            },
        ],
        Box::new(move |answers| {
            let format = answers.get("format").ok_or("Missing format")?;
            let base_url = answers.get("base_url").ok_or("Missing base_url")?;
            let api_key = answers.get("api_key").cloned().unwrap_or_default();
            let models_str = answers.get("models").cloned().unwrap_or_default();

            let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
            let p = config
                .llm
                .providers
                .get_mut(&provider_name)
                .ok_or_else(|| format!("Provider '{}' not found.", provider_name))?;

            p.format = format.clone();
            p.base_url = if base_url.is_empty() {
                None
            } else {
                Some(base_url.clone())
            };
            p.api_key = if api_key.is_empty() {
                None
            } else {
                Some(api_key.clone())
            };
            p.models = models_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let model_info: String = if p.models.is_empty() {
                "(unrestricted)".into()
            } else {
                p.models.join(", ")
            };
            let key_display = if api_key.is_empty() {
                "(not set)".to_string()
            } else if api_key.len() > 8 {
                format!("{}...{}", &api_key[..4], &api_key[api_key.len() - 4..])
            } else {
                "****".to_string()
            };

            config.save().map_err(|e| e.to_string())?;
            Ok(vec![
                format!("Provider '{}' updated!", provider_name),
                format!("  format:   {}", format),
                format!(
                    "  base_url: {}",
                    if base_url.is_empty() {
                        "(default)"
                    } else {
                        base_url.as_str()
                    }
                ),
                format!("  api_key:  {}", key_display),
                format!("  models:   {}", model_info),
                "✓ Applied immediately.".into(),
            ])
        }),
    )
    .with_reload_provider(name.to_string()))
}
