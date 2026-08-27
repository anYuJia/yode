use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexOptions {
    pub max_files: usize,
    pub max_file_bytes: usize,
    pub include_hidden: bool,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            max_files: 20_000,
            max_file_bytes: 512 * 1024,
            include_hidden: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Struct,
    Class,
    Enum,
    Trait,
    Interface,
    Type,
    Module,
    Constant,
    Method,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolRecord {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexedFile {
    pub path: String,
    pub language: String,
    pub line_count: usize,
    pub symbols: Vec<SymbolRecord>,
    pub imports: Vec<String>,
    pub term_frequency: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    pub path: String,
    pub score: f64,
    pub matched_terms: Vec<String>,
    pub symbols: Vec<SymbolRecord>,
    pub imports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexStats {
    pub files: usize,
    pub symbols: usize,
    pub unique_terms: usize,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryIndex {
    pub root: String,
    pub generated_at: String,
    pub files: BTreeMap<String, IndexedFile>,
    #[serde(default)]
    inverted: BTreeMap<String, BTreeSet<String>>,
}

impl RepositoryIndex {
    pub fn build(root: impl AsRef<Path>, options: IndexOptions) -> Result<Self> {
        let root = root.as_ref().canonicalize().with_context(|| {
            format!(
                "failed to resolve repository root {}",
                root.as_ref().display()
            )
        })?;
        let mut index = Self {
            root: root.display().to_string(),
            generated_at: Utc::now().to_rfc3339(),
            files: BTreeMap::new(),
            inverted: BTreeMap::new(),
        };
        let mut candidates = Vec::new();
        collect_files(&root, &root, &options, &mut candidates)?;
        candidates.sort();
        candidates.truncate(options.max_files);
        for path in candidates {
            if let Some(file) = index_file(&root, &path, &options)? {
                index.insert_indexed_file(file);
            }
        }
        Ok(index)
    }

    pub fn stats(&self) -> IndexStats {
        IndexStats {
            files: self.files.len(),
            symbols: self.files.values().map(|file| file.symbols.len()).sum(),
            unique_terms: self.inverted.len(),
            generated_at: self.generated_at.clone(),
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let query_terms = tokenize(query);
        if query_terms.is_empty() {
            return Vec::new();
        }

        let mut candidates = BTreeSet::new();
        for term in &query_terms {
            if let Some(paths) = self.inverted.get(term) {
                candidates.extend(paths.iter().cloned());
            }
        }
        if candidates.is_empty() {
            candidates.extend(self.files.keys().cloned());
        }

        let mut hits = candidates
            .into_iter()
            .filter_map(|path| {
                let file = self.files.get(&path)?;
                let mut score = 0.0;
                let mut matched_terms = Vec::new();
                for term in &query_terms {
                    let frequency = file.term_frequency.get(term).copied().unwrap_or(0);
                    if frequency > 0 {
                        matched_terms.push(term.clone());
                        score += 1.0 + (frequency as f64).ln_1p();
                    }
                    if file
                        .symbols
                        .iter()
                        .any(|symbol| symbol.name.to_ascii_lowercase().contains(term))
                    {
                        score += 4.0;
                    }
                    if file.path.to_ascii_lowercase().contains(term) {
                        score += 2.5;
                    }
                    if file
                        .imports
                        .iter()
                        .any(|import| import.to_ascii_lowercase().contains(term))
                    {
                        score += 1.5;
                    }
                }
                if score <= 0.0 {
                    return None;
                }
                Some(SearchHit {
                    path,
                    score,
                    matched_terms,
                    symbols: file.symbols.iter().take(12).cloned().collect(),
                    imports: file.imports.iter().take(12).cloned().collect(),
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.path.cmp(&right.path))
        });
        hits.truncate(limit.max(1));
        hits
    }

    pub fn update_file(&mut self, relative_path: &str, options: &IndexOptions) -> Result<()> {
        self.remove_file(relative_path);
        let root = Path::new(&self.root);
        let path = root.join(relative_path);
        if path.exists() {
            if let Some(file) = index_file(root, &path, options)? {
                self.insert_indexed_file(file);
            }
        }
        self.generated_at = Utc::now().to_rfc3339();
        Ok(())
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create index directory {}", parent.display())
            })?;
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        fs::write(path, bytes)
            .with_context(|| format!("failed to write repository index {}", path.display()))
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path)
            .with_context(|| format!("failed to read repository index {}", path.display()))?;
        let mut index: Self = serde_json::from_slice(&bytes)?;
        index.rebuild_inverted();
        Ok(index)
    }

    fn insert_indexed_file(&mut self, file: IndexedFile) {
        let path = file.path.clone();
        for term in file.term_frequency.keys() {
            self.inverted
                .entry(term.clone())
                .or_default()
                .insert(path.clone());
        }
        for symbol in &file.symbols {
            for term in tokenize(&symbol.name) {
                self.inverted.entry(term).or_default().insert(path.clone());
            }
        }
        self.files.insert(path, file);
    }

    fn remove_file(&mut self, relative_path: &str) {
        self.files.remove(relative_path);
        for paths in self.inverted.values_mut() {
            paths.remove(relative_path);
        }
        self.inverted.retain(|_, paths| !paths.is_empty());
    }

    fn rebuild_inverted(&mut self) {
        self.inverted.clear();
        let files = self.files.values().cloned().collect::<Vec<_>>();
        self.files.clear();
        for file in files {
            self.insert_indexed_file(file);
        }
    }
}

fn collect_files(
    root: &Path,
    dir: &Path,
    options: &IndexOptions,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if should_skip_dir(&name, options.include_hidden) {
                continue;
            }
            collect_files(root, &path, options, out)?;
        } else if path.is_file() && is_source_file(&path) {
            let metadata = entry.metadata()?;
            if metadata.len() as usize <= options.max_file_bytes {
                out.push(path);
            }
        }
        if out.len() >= options.max_files {
            break;
        }
    }
    let _ = root;
    Ok(())
}

fn should_skip_dir(name: &str, include_hidden: bool) -> bool {
    matches!(
        name,
        ".git" | ".yode" | "target" | "node_modules" | "dist" | "build" | ".next" | "vendor"
    ) || (!include_hidden && name.starts_with('.'))
}

fn is_source_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default(),
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "py"
            | "go"
            | "java"
            | "kt"
            | "kts"
            | "swift"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "dart"
            | "vue"
            | "svelte"
            | "toml"
            | "yaml"
            | "yml"
            | "json"
            | "md"
    )
}

fn index_file(root: &Path, path: &Path, options: &IndexOptions) -> Result<Option<IndexedFile>> {
    let bytes = fs::read(path)?;
    if bytes.len() > options.max_file_bytes || bytes.contains(&0) {
        return Ok(None);
    }
    let content = match String::from_utf8(bytes) {
        Ok(content) => content,
        Err(_) => return Ok(None),
    };
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative = relative.to_string_lossy().replace('\\', "/");
    let language = language_for_path(path).to_string();
    let symbols = extract_symbols(&content, &language);
    let imports = extract_imports(&content, &language);
    let mut term_frequency = BTreeMap::new();
    for term in tokenize(&format!("{}\n{}", relative, content)) {
        *term_frequency.entry(term).or_insert(0) += 1;
    }
    Ok(Some(IndexedFile {
        path: relative,
        language,
        line_count: content.lines().count(),
        symbols,
        imports,
        term_frequency,
    }))
}

fn language_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cc" | "cpp" | "hpp" => "cpp",
        "dart" => "dart",
        "vue" => "vue",
        "svelte" => "svelte",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "md" => "markdown",
        _ => "text",
    }
}

fn extract_symbols(content: &str, language: &str) -> Vec<SymbolRecord> {
    let patterns: &[(&str, SymbolKind)] = match language {
        "rust" => &[
            (
                r"^(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
                SymbolKind::Function,
            ),
            (
                r"^(?:pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)",
                SymbolKind::Struct,
            ),
            (
                r"^(?:pub\s+)?enum\s+([A-Za-z_][A-Za-z0-9_]*)",
                SymbolKind::Enum,
            ),
            (
                r"^(?:pub\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)",
                SymbolKind::Trait,
            ),
            (
                r"^(?:pub\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)",
                SymbolKind::Module,
            ),
            (
                r"^(?:pub\s+)?const\s+([A-Za-z_][A-Za-z0-9_]*)",
                SymbolKind::Constant,
            ),
        ],
        "typescript" | "javascript" | "vue" | "svelte" => &[
            (
                r"^(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][A-Za-z0-9_$]*)",
                SymbolKind::Function,
            ),
            (
                r"^(?:export\s+)?class\s+([A-Za-z_$][A-Za-z0-9_$]*)",
                SymbolKind::Class,
            ),
            (
                r"^(?:export\s+)?interface\s+([A-Za-z_$][A-Za-z0-9_$]*)",
                SymbolKind::Interface,
            ),
            (
                r"^(?:export\s+)?type\s+([A-Za-z_$][A-Za-z0-9_$]*)",
                SymbolKind::Type,
            ),
            (
                r"^(?:export\s+)?const\s+([A-Za-z_$][A-Za-z0-9_$]*)",
                SymbolKind::Constant,
            ),
        ],
        "python" => &[
            (
                r"^(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)",
                SymbolKind::Function,
            ),
            (r"^class\s+([A-Za-z_][A-Za-z0-9_]*)", SymbolKind::Class),
        ],
        "go" => &[
            (
                r"^func\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_]*)",
                SymbolKind::Function,
            ),
            (
                r"^type\s+([A-Za-z_][A-Za-z0-9_]*)\s+struct",
                SymbolKind::Struct,
            ),
            (
                r"^type\s+([A-Za-z_][A-Za-z0-9_]*)\s+interface",
                SymbolKind::Interface,
            ),
        ],
        _ => &[],
    };

    let compiled = patterns
        .iter()
        .filter_map(|(pattern, kind)| Regex::new(pattern).ok().map(|regex| (regex, *kind)))
        .collect::<Vec<_>>();
    let mut symbols = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        for (regex, kind) in &compiled {
            if let Some(captures) = regex.captures(trimmed) {
                if let Some(name) = captures.get(1) {
                    symbols.push(SymbolRecord {
                        name: name.as_str().to_string(),
                        kind: *kind,
                        line: line_index + 1,
                    });
                    break;
                }
            }
        }
    }
    symbols
}

fn extract_imports(content: &str, language: &str) -> Vec<String> {
    let mut imports = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let candidate = match language {
            "rust" if trimmed.starts_with("use ") || trimmed.starts_with("mod ") => Some(trimmed),
            "typescript" | "javascript" | "vue" | "svelte"
                if trimmed.starts_with("import ") || trimmed.contains("require(") =>
            {
                Some(trimmed)
            }
            "python" if trimmed.starts_with("import ") || trimmed.starts_with("from ") => {
                Some(trimmed)
            }
            "go" if trimmed.starts_with('"') && trimmed.ends_with('"') => Some(trimmed),
            _ => None,
        };
        if let Some(candidate) = candidate {
            imports.push(candidate.chars().take(240).collect());
        }
        if imports.len() >= 80 {
            break;
        }
    }
    imports
}

fn tokenize(input: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            if current.len() >= 2 {
                terms.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }
    if current.len() >= 2 {
        terms.push(current);
    }
    terms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_searches_symbols() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/auth.rs"),
            "pub struct SessionStore {}\npub fn load_session() {}\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/main.rs"),
            "mod auth;\nfn main() { auth::load_session(); }\n",
        )
        .unwrap();

        let index = RepositoryIndex::build(dir.path(), IndexOptions::default()).unwrap();
        assert_eq!(index.stats().files, 2);
        let hits = index.search("SessionStore load_session", 5);
        assert_eq!(hits[0].path, "src/auth.rs");
        assert!(hits[0].score > 0.0);
    }

    #[test]
    fn incremental_update_replaces_file_terms() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("one.rs"), "fn old_symbol() {}\n").unwrap();
        let options = IndexOptions::default();
        let mut index = RepositoryIndex::build(dir.path(), options.clone()).unwrap();
        assert!(!index.search("old_symbol", 5).is_empty());

        fs::write(dir.path().join("one.rs"), "fn new_symbol() {}\n").unwrap();
        index.update_file("one.rs", &options).unwrap();
        assert!(index.search("old_symbol", 5).is_empty());
        assert!(!index.search("new_symbol", 5).is_empty());
    }

    #[test]
    fn persisted_index_rebuilds_inverted_lookup() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("one.py"), "def authenticate():\n    pass\n").unwrap();
        let index = RepositoryIndex::build(dir.path(), IndexOptions::default()).unwrap();
        let path = dir.path().join("index.json");
        index.save_json(&path).unwrap();
        let loaded = RepositoryIndex::load_json(&path).unwrap();
        assert_eq!(loaded.search("authenticate", 3)[0].path, "one.py");
    }
}
