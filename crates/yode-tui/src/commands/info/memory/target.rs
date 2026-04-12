use super::compare::{parse_compare_args, CompareArgs};
use super::transcripts::{parse_list_filter, parse_latest_compare_target, TranscriptListFilter};

pub(super) enum MemoryTarget {
    Overview,
    Live,
    Session,
    Picker,
    List(TranscriptListFilter),
    Compare(CompareArgs),
    Latest,
    LatestCompare(String),
    Transcript(String),
}

pub(super) fn parse_memory_target(args: &str) -> Result<MemoryTarget, String> {
    let args = args.trim();
    if args == "compare" || args.starts_with("compare ") {
        let compare = parse_compare_args(args).ok_or_else(|| {
            "Usage: /memory compare <a> <b> [--no-diff] [--hunks N] [--lines N]".to_string()
        })?;
        return Ok(MemoryTarget::Compare(compare));
    }
    if args == "list" || args.starts_with("list ") {
        return Ok(MemoryTarget::List(parse_list_filter(args)?));
    }

    match args {
        "" => Ok(MemoryTarget::Overview),
        "live" => Ok(MemoryTarget::Live),
        "session" => Ok(MemoryTarget::Session),
        "pick" => Ok(MemoryTarget::Picker),
        "latest" => Ok(MemoryTarget::Latest),
        _ if args.starts_with("latest compare ") => parse_latest_compare_target(args)
            .map(|target| MemoryTarget::LatestCompare(target.to_string()))
            .ok_or_else(|| "Usage: /memory latest compare <target>".to_string()),
        target => Ok(MemoryTarget::Transcript(target.to_string())),
    }
}
