#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::commands::info::memory) struct CompareArgs {
    pub left_target: String,
    pub right_target: String,
    pub options: CompareOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::commands::info::memory) struct CompareOptions {
    pub diff_enabled: bool,
    pub max_hunks: usize,
    pub max_lines: usize,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            diff_enabled: true,
            max_hunks: 3,
            max_lines: 60,
        }
    }
}

pub(in crate::commands::info::memory) fn parse_compare_args(args: &str) -> Option<CompareArgs> {
    let rest = args.strip_prefix("compare ")?;
    let tokens = rest.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 2 {
        return None;
    }
    let mut compare = CompareArgs {
        left_target: tokens[0].to_string(),
        right_target: tokens[1].to_string(),
        options: CompareOptions::default(),
    };

    let mut index = 2usize;
    while index < tokens.len() {
        match tokens[index] {
            "--no-diff" => {
                compare.options.diff_enabled = false;
                index += 1;
            }
            "--hunks" => {
                let value = tokens.get(index + 1)?.parse::<usize>().ok()?;
                if value == 0 {
                    return None;
                }
                compare.options.max_hunks = value;
                index += 2;
            }
            "--lines" => {
                let value = tokens.get(index + 1)?.parse::<usize>().ok()?;
                if value == 0 {
                    return None;
                }
                compare.options.max_lines = value;
                index += 2;
            }
            _ => return None,
        }
    }

    Some(compare)
}
