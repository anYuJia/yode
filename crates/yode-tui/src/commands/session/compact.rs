use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct CompactCommand {
    meta: CommandMeta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompactRequest {
    Full,
    KeepLast(usize),
    From(usize),
    UpTo(usize),
}

const COMPACT_USAGE: &str =
    "用法：/compact [keep_last=20 | keep-last 20 | from=40 | from 40 | up_to=80 | up-to 80]";
const COMPACT_COMPLETIONS: &[&str] = &[
    "full",
    "keep_last=20",
    "keep-last 20",
    "from=40",
    "from 40",
    "up_to=80",
    "up-to 80",
    "help",
];

impl CompactCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "compact",
                description: "Compact chat history to recent entries",
                aliases: &[],
                args: vec![ArgDef {
                    name: "range".to_string(),
                    required: false,
                    hint: "[full | keep_last=20 | from=40 | up_to=80]".to_string(),
                    completions: ArgCompletionSource::Static(
                        COMPACT_COMPLETIONS
                            .iter()
                            .map(|value| value.to_string())
                            .collect(),
                    ),
                }],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for CompactCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let engine = ctx.engine.clone();
        let event_tx = ctx.engine_event_tx.clone();
        let request = match parse_compact_request(args) {
            Ok(request) => request,
            Err(err) => {
                return Ok(CommandOutput::Message(format!(
                    "{}\n{}",
                    err, COMPACT_USAGE
                )));
            }
        };

        tokio::spawn(async move {
            let mut engine = engine.lock().await;
            let changed = match request {
                CompactRequest::Full => engine.force_compact(event_tx.clone()).await,
                CompactRequest::KeepLast(keep_last) => {
                    engine
                        .force_compact_keep_last(keep_last, event_tx.clone())
                        .await
                }
                CompactRequest::From(from) => {
                    engine
                        .force_partial_compact_from(from, event_tx.clone())
                        .await
                }
                CompactRequest::UpTo(up_to) => {
                    engine
                        .force_partial_compact_up_to(up_to, event_tx.clone())
                        .await
                }
            };
            if !changed {
                let _ = event_tx.send(yode_core::engine::EngineEvent::Error(
                    "压缩未产生变化：当前会话太短，或已经低于压缩目标，因此没有写入新的压缩记录。"
                        .to_string(),
                ));
            }
        });

        let summary = match request {
            CompactRequest::Full => "已请求完整压缩上下文。".to_string(),
            CompactRequest::KeepLast(keep_last) => {
                format!("已请求压缩上下文（keep_last={}）。", keep_last)
            }
            CompactRequest::From(from) => {
                format!("已请求部分压缩上下文（from={}）。", from)
            }
            CompactRequest::UpTo(up_to) => {
                format!("已请求部分压缩上下文（up_to={}）。", up_to)
            }
        };

        Ok(CommandOutput::Message(summary))
    }
}

fn parse_compact_request(args: &str) -> Result<CompactRequest, String> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return Ok(CompactRequest::Full);
    }

    if matches!(trimmed, "help" | "--help" | "-h") {
        return Err("显示 compact 参数帮助。".to_string());
    }

    if matches!(trimmed, "full" | "all") {
        return Ok(CompactRequest::Full);
    }

    for (prefix, request) in [
        (
            "keep_last=",
            CompactRequest::KeepLast as fn(usize) -> CompactRequest,
        ),
        ("keep-last=", CompactRequest::KeepLast),
        ("last=", CompactRequest::KeepLast),
        ("from=", CompactRequest::From),
        ("up_to=", CompactRequest::UpTo),
        ("up-to=", CompactRequest::UpTo),
    ] {
        if let Some(value) = trimmed.strip_prefix(prefix) {
            return parse_positive_usize(value, prefix.trim_end_matches('=')).map(request);
        }
    }

    let parts = trimmed.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        [value] => parse_positive_usize(value, "keep_last").map(CompactRequest::KeepLast),
        ["keep_last" | "keep-last" | "last", value] => {
            parse_positive_usize(value, "keep_last").map(CompactRequest::KeepLast)
        }
        ["from", value] => parse_positive_usize(value, "from").map(CompactRequest::From),
        ["up_to" | "up-to", value] => {
            parse_positive_usize(value, "up_to").map(CompactRequest::UpTo)
        }
        _ => Err(format!("无法解析 compact 参数：{}", trimmed)),
    }
}

fn parse_positive_usize(value: &str, label: &str) -> Result<usize, String> {
    let parsed = value
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("{} 必须是正整数：{}", label, value.trim()))?;
    if parsed == 0 {
        return Err(format!("{} 必须大于 0。", label));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::{parse_compact_request, CompactRequest};

    #[test]
    fn parses_keep_last_argument() {
        assert_eq!(parse_compact_request(""), Ok(CompactRequest::Full));
        assert_eq!(parse_compact_request("full"), Ok(CompactRequest::Full));
        assert_eq!(
            parse_compact_request("12"),
            Ok(CompactRequest::KeepLast(12))
        );
        assert_eq!(
            parse_compact_request("keep_last=24"),
            Ok(CompactRequest::KeepLast(24))
        );
        assert_eq!(
            parse_compact_request("keep-last 24"),
            Ok(CompactRequest::KeepLast(24))
        );
        assert_eq!(
            parse_compact_request("from=18"),
            Ok(CompactRequest::From(18))
        );
        assert_eq!(
            parse_compact_request("from 18"),
            Ok(CompactRequest::From(18))
        );
        assert_eq!(
            parse_compact_request("up_to=32"),
            Ok(CompactRequest::UpTo(32))
        );
        assert_eq!(
            parse_compact_request("up-to 32"),
            Ok(CompactRequest::UpTo(32))
        );
        assert!(parse_compact_request("keep_last=0").is_err());
        assert!(parse_compact_request("abc").is_err());
        assert!(parse_compact_request("from nope").is_err());
    }
}
