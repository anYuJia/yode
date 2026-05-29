use std::sync::LazyLock;

static EN: LazyLock<toml::Value> =
    LazyLock::new(|| parse_locale(include_str!("../../../i18n/en.toml")));
static ZH_CN: LazyLock<toml::Value> =
    LazyLock::new(|| parse_locale(include_str!("../../../i18n/zh-CN.toml")));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Locale {
    En,
    ZhCn,
}

pub(crate) fn current_locale() -> Locale {
    let value = std::env::var("YODE_LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if value.starts_with("zh") {
        Locale::ZhCn
    } else {
        Locale::En
    }
}

pub(crate) fn text(key: &str) -> String {
    text_for(current_locale(), key).unwrap_or(key).to_string()
}

pub(crate) fn text_or(key: &str, fallback: &'static str) -> String {
    text_for(current_locale(), key)
        .unwrap_or(fallback)
        .to_string()
}

pub(crate) fn text_for(locale: Locale, key: &str) -> Option<&'static str> {
    let root = match locale {
        Locale::En => &*EN,
        Locale::ZhCn => &*ZH_CN,
    };
    lookup_text(root, key)
}

fn parse_locale(content: &str) -> toml::Value {
    content
        .parse::<toml::Value>()
        .unwrap_or_else(|_| toml::Value::Table(Default::default()))
}

fn lookup_text(root: &'static toml::Value, key: &str) -> Option<&'static str> {
    let mut value = root;
    for part in key.split('.') {
        value = value.get(part)?;
    }
    value.as_str()
}

#[cfg(test)]
mod tests {
    use super::{text_for, text_or, Locale};

    #[test]
    fn loads_core_ui_text_from_toml_resources() {
        assert_eq!(
            text_for(Locale::En, "ui.input_ask_anything"),
            Some("Ask anything…")
        );
        assert_eq!(
            text_for(Locale::ZhCn, "ui.input_ask_anything"),
            Some("输入你的需求…")
        );
        assert_eq!(text_for(Locale::En, "missing.key"), None);
        assert_eq!(text_or("missing.key", "fallback"), "fallback");
    }
}
