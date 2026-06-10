use crate::config::{Config, ProviderConfig};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io::{self, Write};

/// Check if any API key is configured either in ENV or Config
pub fn has_api_keys_configured() -> bool {
    let has_env = std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("ANTHROPIC_API_KEY").is_ok()
        || std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok();

    let has_config = if let Ok(config) = Config::load() {
        config.llm.providers.values().any(|p| p.api_key.is_some())
    } else {
        false
    };

    has_env || has_config
}

/// Shared Option struct
pub struct MenuOption {
    value: &'static str,
    title: &'static str,
}

impl MenuOption {
    pub fn new(value: &'static str, title: &'static str) -> Self {
        Self { value, title }
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderSetupDefaults {
    format: &'static str,
    base_url: &'static str,
    name: &'static str,
    model: &'static str,
}

fn provider_setup_defaults(provider: &str) -> ProviderSetupDefaults {
    match provider {
        "anthropic" => ProviderSetupDefaults {
            format: "anthropic",
            base_url: "https://api.anthropic.com",
            name: "anthropic",
            model: "claude-sonnet-4-20250514",
        },
        "openai" => ProviderSetupDefaults {
            format: "openai",
            base_url: "https://api.openai.com/v1",
            name: "openai",
            model: "gpt-4o",
        },
        "kimi" => ProviderSetupDefaults {
            format: "openai",
            base_url: "https://api.moonshot.cn/v1",
            name: "kimi",
            model: "moonshot-v1-auto",
        },
        "deepseek" => ProviderSetupDefaults {
            format: "openai",
            base_url: "https://api.deepseek.com",
            name: "deepseek",
            model: "deepseek-chat",
        },
        "gemini" => ProviderSetupDefaults {
            format: "openai",
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai/",
            name: "gemini",
            model: "gemini-2.5-flash",
        },
        _ => ProviderSetupDefaults {
            format: "custom",
            base_url: "",
            name: "custom",
            model: "gpt-4o",
        },
    }
}

pub fn run_setup_interactive() -> Result<()> {
    let options = vec![
        MenuOption::new("anthropic", "Anthropic (Claude) - 推荐"),
        MenuOption::new("openai", "OpenAI (GPT)"),
        MenuOption::new("kimi", "Kimi (Moonshot)"),
        MenuOption::new("deepseek", "DeepSeek"),
        MenuOption::new("gemini", "Google Gemini"),
        MenuOption::new("custom", "自定义 (Custom)"),
    ];

    let header = "╔═══════════════════════════════════════════════════════════╗\n║                    Yode 首次配置                          ║\n║         需要配置 LLM API 才能开始使用 Yode                 ║\n╚═══════════════════════════════════════════════════════════╝";
    let selected = select_menu(Some(header), "\n请选择要添加的 LLM 提供商", &options)?;
    let option = &options[selected];

    let mut config = Config::load().or_else(|_| load_default_config())?;

    let defaults = provider_setup_defaults(option.value);

    println!("\n正在配置 [{}]...", option.title);

    let format_val = if defaults.format == "custom" {
        let fmt_options = vec![
            MenuOption::new("openai", "OpenAI 兼容格式 (绝大部分自建或平台适用)"),
            MenuOption::new("anthropic", "Anthropic 兼容格式"),
        ];
        let idx = select_menu(None, "\n请选择接口格式的兼容标准", &fmt_options)?;
        fmt_options[idx].value.to_string()
    } else {
        defaults.format.to_string()
    };

    let p_base_url = if defaults.format == "custom" {
        let prompt = "请输入 Base URL (例如 https://api.openai.com/v1): ";
        let mut u = read_input(prompt)?;
        while u.is_empty() {
            println!("自定义模式必须输入 Base URL");
            u = read_input(prompt)?;
        }
        u
    } else {
        let prompt = format!(
            "请输入 Base URL (直接回车使用官方默认 {}): ",
            defaults.base_url
        );
        let u = read_input(&prompt)?;
        if u.is_empty() {
            defaults.base_url.to_string()
        } else {
            u
        }
    };

    let p_api_key = loop {
        let k = read_input("请输入 API Key: ")?;
        if k.is_empty() {
            println!("API Key 不能为空！");
        } else {
            break k;
        }
    };

    let prompt = format!(
        "请为该 Provider 起个名字 (直接回车使用默认 '{}'): ",
        defaults.name
    );
    let mut p_name = read_input(&prompt)?;
    if p_name.is_empty() {
        p_name = defaults.name.to_string();
    }

    config.llm.providers.insert(
        p_name.clone(),
        ProviderConfig {
            format: format_val,
            base_url: Some(p_base_url),
            api_key: Some(p_api_key.clone()),
            models: Vec::new(),
            enabled: None,
            gradient: None,
        },
    );

    config.llm.default_provider = p_name.clone();

    let prompt = format!(
        "请输入此 Provider 默认使用的模型名称 (直接回车推荐 '{}'): ",
        defaults.model
    );
    let mut m_name = read_input(&prompt)?;
    if m_name.is_empty() {
        m_name = defaults.model.to_string();
    }
    config.llm.default_model = m_name;

    config.save()?;

    println!("\n✓ 配置已成功保存！当前 Provider: {}", p_name);
    println!("\n按任意键继续启动 Yode...");
    wait_for_key()?;

    Ok(())
}

fn select_menu(header: Option<&str>, prompt: &str, options: &[MenuOption]) -> Result<usize> {
    let mut selected = 0;

    if let Some(h) = header {
        println!("\n{}", h);
    }
    println!("{} (使用 ↑↓ 切换，回车确认):\n", prompt);

    let raw_mode = RawModeGuard::enable()?;

    let result = loop {
        for (i, option) in options.iter().enumerate() {
            if i == selected {
                print!("\r  \x1B[32m> {}. {}\x1B[0m\x1B[K\r\n", i + 1, option.title);
            } else {
                print!("\r    {}. {}\x1B[K\r\n", i + 1, option.title);
            }
        }

        print!("\r\x1B[{}A", options.len());
        io::stdout().flush()?;

        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = event::read()?
        {
            if kind == KeyEventKind::Press {
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                    drop(raw_mode);
                    std::process::exit(0);
                }
                match code {
                    KeyCode::Up => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down if selected < options.len() - 1 => {
                        selected += 1;
                    }
                    KeyCode::Enter => {
                        break Ok(selected);
                    }
                    KeyCode::Esc => {
                        drop(raw_mode);
                        std::process::exit(0);
                    }
                    _ => {}
                }
            }
        }
    };

    drop(raw_mode);

    println!("\x1B[{}B", options.len());
    io::stdout().flush()?;

    result
}

fn read_input(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn load_default_config() -> Result<Config> {
    let default_str = include_str!("../../../config/default.toml");
    Ok(toml::from_str(default_str)?)
}

fn wait_for_key() -> Result<()> {
    let raw_mode = RawModeGuard::enable()?;
    loop {
        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event::read()?
        {
            if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                drop(raw_mode);
                std::process::exit(0);
            }
            break;
        }
    }
    drop(raw_mode);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_default_config, provider_setup_defaults};

    #[test]
    fn provider_setup_defaults_cover_builtin_providers() {
        let anthropic = provider_setup_defaults("anthropic");
        assert_eq!(anthropic.format, "anthropic");
        assert_eq!(anthropic.base_url, "https://api.anthropic.com");
        assert_eq!(anthropic.model, "claude-sonnet-4-20250514");

        let gemini = provider_setup_defaults("gemini");
        assert_eq!(gemini.format, "openai");
        assert!(gemini
            .base_url
            .contains("generativelanguage.googleapis.com"));
        assert_eq!(gemini.model, "gemini-2.5-flash");
    }

    #[test]
    fn provider_setup_defaults_fall_back_to_custom() {
        let custom = provider_setup_defaults("local");
        assert_eq!(custom.format, "custom");
        assert_eq!(custom.base_url, "");
        assert_eq!(custom.name, "custom");
        assert_eq!(custom.model, "gpt-4o");
    }

    #[test]
    fn default_setup_config_parses_without_panicking() {
        let config = load_default_config().unwrap();
        assert!(!config.llm.default_provider.is_empty());
        assert!(!config.llm.default_model.is_empty());
    }
}
