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
                    hint: "[keep_last=20 | from=40 | up_to=80]".to_string(),
                    completions: ArgCompletionSource::None,
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
        let request = parse_compact_request(args).unwrap_or(CompactRequest::Full);

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

fn parse_compact_request(args: &str) -> Option<CompactRequest> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return Some(CompactRequest::Full);
    }

    if let Some(value) = trimmed.strip_prefix("keep_last=") {
        return value
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .map(CompactRequest::KeepLast);
    }

    if let Some(value) = trimmed.strip_prefix("from=") {
        return value
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .map(CompactRequest::From);
    }

    if let Some(value) = trimmed.strip_prefix("up_to=") {
        return value
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .map(CompactRequest::UpTo);
    }

    trimmed
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .map(CompactRequest::KeepLast)
}

#[cfg(test)]
mod tests {
    use super::{parse_compact_request, CompactRequest};

    #[test]
    fn parses_keep_last_argument() {
        assert_eq!(parse_compact_request(""), Some(CompactRequest::Full));
        assert_eq!(
            parse_compact_request("12"),
            Some(CompactRequest::KeepLast(12))
        );
        assert_eq!(
            parse_compact_request("keep_last=24"),
            Some(CompactRequest::KeepLast(24))
        );
        assert_eq!(
            parse_compact_request("from=18"),
            Some(CompactRequest::From(18))
        );
        assert_eq!(
            parse_compact_request("up_to=32"),
            Some(CompactRequest::UpTo(32))
        );
        assert_eq!(parse_compact_request("keep_last=0"), None);
    }
}
