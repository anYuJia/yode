mod args;
mod output;
mod section_stats;

pub(in crate::commands::info::memory) use args::{parse_compare_args, CompareArgs, CompareOptions};
pub(in crate::commands::info::memory) use output::build_transcript_compare_output;
