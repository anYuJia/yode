mod instruction_loader;
mod memory_loader;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_INSTRUCTION_CHARS: usize = 40_000;
const MAX_MEMORY_BYTES: usize = 25_000;
const MAX_MEMORY_LINES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InstructionLayer {
    GlobalAdmin,
    User,
    Project,
    Local,
}

impl InstructionLayer {
    fn title(self) -> &'static str {
        match self {
            Self::GlobalAdmin => "Global Admin Instructions",
            Self::User => "User Instructions",
            Self::Project => "Project Instructions",
            Self::Local => "Local Instructions",
        }
    }
}

#[derive(Debug, Clone)]
struct InstructionEntry {
    layer: InstructionLayer,
    path: PathBuf,
}

#[derive(Debug, Default)]
struct LoadState {
    visited: HashSet<PathBuf>,
    total_chars: usize,
    truncated: bool,
}

pub use self::instruction_loader::load_instruction_context;
pub use self::memory_loader::load_memory_context;

#[cfg(test)]
mod tests;
