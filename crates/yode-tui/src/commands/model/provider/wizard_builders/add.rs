use crate::app::wizard::{Wizard, WizardCompletion, WizardStep};

use super::super::config_ops::add_provider_to_config;

pub(crate) fn build_add_provider_wizard() -> Wizard {
    let default_preset = provider_type_defaults("Anthropic (Claude)");
    let initial_model_picker_step = build_add_model_picker_step(
        default_preset
            .as_ref()
            .map(|preset| preset.default_models.as_slice())
            .unwrap_or(&[]),
    );
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
            initial_model_picker_step,
            WizardStep::Input {
                prompt: "模型列表（可用 , 分隔多个；第一个会作为默认模型）:".into(),
                default: Some("claude-sonnet-4-20250514".into()),
                key: "models".into(),
            },
        ],
        Box::new(|answers| {
            let provider_type = answers.get("provider_type").ok_or("Missing type")?;
            let name = answers.get("name").ok_or("Missing name")?;
            let base_url = answers.get("base_url").ok_or("Missing base_url")?;
            let api_key = answers.get("api_key").ok_or("Missing api_key")?;
            let models_str = normalize_wizard_model_answer(answers.get("models"));

            let format = if provider_type.contains("Anthropic") {
                "anthropic"
            } else if provider_type.contains("Gemini") {
                "gemini"
            } else {
                "openai"
            };

            let models = if models_str.is_empty() {
                vec![]
            } else {
                models_str
                    .split(',')
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>()
            };

            add_provider_to_config(
                name,
                format,
                Some(base_url.as_str()),
                &models,
                Some(api_key),
            )?;

            Ok(WizardCompletion::messages(vec![
                format!("✓ Provider '{}' 已添加!", name),
                format!("  format:   {}", format),
                format!("  base_url: {}", base_url),
                format!(
                    "  models:   {}",
                    if models_str.is_empty() {
                        "(unrestricted)"
                    } else {
                        &models_str
                    }
                ),
                format!(
                    "  default:  {}",
                    models.first().map(String::as_str).unwrap_or("(none)")
                ),
                format!(
                    "  api_key:  {}...{}",
                    &api_key[..4.min(api_key.len())],
                    &api_key[api_key.len().saturating_sub(4)..]
                ),
                String::new(),
                "重启 yode 以激活新提供商。".into(),
            ]))
        }),
    )
    .with_step_callback(Box::new(|value, steps| {
        if let Some(preset) = provider_type_defaults(value) {
            if let Some(WizardStep::Input { default, .. }) = steps.get_mut(1) {
                *default = Some(preset.default_url.into());
            }
            if let Some(WizardStep::Input { default, .. }) = steps.get_mut(3) {
                *default = Some(preset.name_hint.into());
            }
            if let Some(step) = steps.get_mut(4) {
                *step = build_add_model_picker_step(&preset.default_models);
            }
            if let Some(WizardStep::Input { default, .. }) = steps.get_mut(5) {
                *default = Some(
                    preset
                        .default_models
                        .first()
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            return;
        }

        if let Some(WizardStep::Input { default, .. }) = steps.get_mut(5) {
            *default = Some(match value.trim() {
                "(custom input)" => default.clone().unwrap_or_default(),
                "(unrestricted)" => String::new(),
                other => other.to_string(),
            });
        }
    }))
}

#[derive(Debug, Clone)]
struct ProviderTypePreset {
    default_url: &'static str,
    name_hint: &'static str,
    default_models: Vec<String>,
}

fn provider_type_defaults(value: &str) -> Option<ProviderTypePreset> {
    let (default_url, name_hint, fallback_model_hint) = match value {
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
        v if v.contains("DeepSeek") => ("https://api.deepseek.com/v1", "deepseek", "deepseek-chat"),
        v if v.contains("Qwen") || v.contains("千问") => (
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "qwen",
            "qwen-max",
        ),
        v if v.contains("Zhipu") || v.contains("智谱") => (
            "https://open.bigmodel.cn/api/paas/v4",
            "zhipu",
            "glm-5",
        ),
        v if v.contains("Kimi") || v.contains("月之暗面") => (
            "https://api.moonshot.cn/v1",
            "moonshot",
            "kimi-k2.5",
        ),
        v if v.contains("Doubao") || v.contains("豆包") => (
            "https://ark.cn-beijing.volces.com/api/v3",
            "doubao",
            "doubao-pro-256k",
        ),
        v if v.contains("SiliconFlow") || v.contains("硅基") => (
            "https://api.siliconflow.cn/v1",
            "siliconflow",
            "deepseek-v3",
        ),
        v if v.contains("Yi") || v.contains("零一") => (
            "https://api.lingyiwanwu.com/v1",
            "yi",
            "yi-lightning",
        ),
        v if v.contains("Baichuan") || v.contains("百川") => (
            "https://api.baichuan-ai.com/v1",
            "baichuan",
            "Baichuan4",
        ),
        v if v.contains("Spark") || v.contains("星火") => (
            "https://spark-api-open.xf-yun.com/v1",
            "spark",
            "generalv3.5",
        ),
        v if v.contains("MiniMax") => (
            "https://api.minimax.chat/v1",
            "minimax",
            "MiniMax-M2.7",
        ),
        v if v.contains("StepFun") || v.contains("阶跃") => (
            "https://api.stepfun.com/v1",
            "stepfun",
            "step-3.5-flash",
        ),
        v if v.contains("ERNIE") || v.contains("文心") => (
            "https://qianfan.baidubce.com/v2",
            "ernie",
            "ernie-4.0-8k",
        ),
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
        v if v.contains("自定义") || v.contains("Custom") => ("", "custom", ""),
        _ => return None,
    };

    let default_models = yode_llm::find_provider_info(name_hint)
        .map(|info| info.default_models.iter().map(|item| item.to_string()).collect::<Vec<_>>())
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| {
            if fallback_model_hint.is_empty() {
                Vec::new()
            } else {
                vec![fallback_model_hint.to_string()]
            }
        });

    Some(ProviderTypePreset {
        default_url,
        name_hint,
        default_models,
    })
}

fn build_add_model_picker_step(default_models: &[String]) -> WizardStep {
    if default_models.is_empty() {
        WizardStep::Select {
            prompt: "选择模型预设:".into(),
            options: vec!["(custom input)".into(), "(unrestricted)".into()],
            default: 0,
            key: "model_picker".into(),
        }
    } else {
        let mut options = default_models.to_vec();
        options.push("(custom input)".into());
        options.push("(unrestricted)".into());
        WizardStep::Select {
            prompt: "选择模型预设:".into(),
            options,
            default: 0,
            key: "model_picker".into(),
        }
    }
}

fn normalize_wizard_model_answer(value: Option<&String>) -> String {
    match value.map(|item| item.trim()) {
        Some("(unrestricted)") | None => String::new(),
        Some(other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_add_provider_wizard, provider_type_defaults};
    use crate::app::wizard::WizardStep;

    #[test]
    fn provider_type_defaults_surface_known_models() {
        let preset = provider_type_defaults("Anthropic (Claude)").unwrap();
        assert_eq!(preset.name_hint, "anthropic");
        assert!(preset.default_models.iter().any(|item| item.contains("claude-sonnet")));
    }

    #[test]
    fn add_wizard_turns_model_step_into_select_for_known_provider() {
        let mut wizard = build_add_provider_wizard();
        assert!(matches!(wizard.steps[4], WizardStep::Select { .. }));
        let _ = wizard.submit().unwrap();
        assert!(matches!(wizard.steps[4], WizardStep::Select { .. }));
        assert!(matches!(wizard.steps[5], WizardStep::Input { .. }));
    }
}
