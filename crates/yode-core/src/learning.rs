use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

const MAX_POSTMORTEMS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunPostmortem {
    pub run_id: String,
    pub session_id: String,
    pub goal: String,
    pub success: bool,
    pub final_phase: String,
    #[serde(default)]
    pub model_routes: Vec<String>,
    #[serde(default)]
    pub failed_tools: Vec<String>,
    #[serde(default)]
    pub verification_evidence: Vec<String>,
    #[serde(default)]
    pub modified_files: Vec<String>,
    #[serde(default)]
    pub replans: u32,
    #[serde(default)]
    pub compactions: u32,
    #[serde(default)]
    pub subagents: u32,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub lessons: Vec<String>,
    pub recorded_at: String,
}

impl RunPostmortem {
    pub fn new(run_id: impl Into<String>, session_id: impl Into<String>, goal: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            session_id: session_id.into(),
            goal: goal.into(),
            success: false,
            final_phase: "unknown".to_string(),
            model_routes: Vec::new(),
            failed_tools: Vec::new(),
            verification_evidence: Vec::new(),
            modified_files: Vec::new(),
            replans: 0,
            compactions: 0,
            subagents: 0,
            summary: String::new(),
            lessons: Vec::new(),
            recorded_at: Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LearnedLesson {
    pub key: String,
    pub text: String,
    pub observations: u32,
    pub successes_after_learning: u32,
    pub confidence: f32,
    pub last_seen_at: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LearningSummary {
    pub postmortems: usize,
    pub lessons: usize,
    pub recurring_failure_patterns: usize,
}

#[derive(Debug, Clone)]
pub struct LearningStore {
    root: PathBuf,
}

impl LearningStore {
    pub fn for_workspace(workspace: impl AsRef<Path>) -> Self {
        Self { root: workspace.as_ref().join(".yode").join("learning") }
    }

    pub fn record(&self, mut postmortem: RunPostmortem) -> Result<Vec<LearnedLesson>> {
        fs::create_dir_all(&self.root)?;
        sanitize_postmortem(&mut postmortem);
        if postmortem.recorded_at.trim().is_empty() {
            postmortem.recorded_at = Utc::now().to_rfc3339();
        }
        let postmortem_path = self.root.join("postmortems.jsonl");
        let mut file = OpenOptions::new().create(true).append(true).open(&postmortem_path)?;
        serde_json::to_writer(&mut file, &postmortem)?;
        file.write_all(b"\n")?;
        file.flush()?;

        let mut lessons = self.load_all_lessons()?;
        for (key, text, tags) in derive_lessons(&postmortem) {
            let entry = lessons.entry(key.clone()).or_insert_with(|| LearnedLesson {
                key,
                text: text.clone(),
                observations: 0,
                successes_after_learning: 0,
                confidence: 0.0,
                last_seen_at: postmortem.recorded_at.clone(),
                tags: tags.clone(),
            });
            entry.text = text;
            entry.observations = entry.observations.saturating_add(1);
            if postmortem.success {
                entry.successes_after_learning = entry.successes_after_learning.saturating_add(1);
            }
            entry.last_seen_at = postmortem.recorded_at.clone();
            entry.tags.extend(tags);
            entry.tags.sort();
            entry.tags.dedup();
            entry.confidence = confidence(entry.observations, entry.successes_after_learning);
        }
        self.save_lessons(&lessons)?;
        self.prune_postmortems(MAX_POSTMORTEMS)?;
        Ok(lessons.into_values().collect())
    }

    pub fn relevant_lessons(&self, query: &str, limit: usize) -> Result<Vec<LearnedLesson>> {
        let query_terms = terms(query);
        let mut lessons = self.load_all_lessons()?.into_values().collect::<Vec<_>>();
        lessons.sort_by(|left, right| {
            let right_score = lesson_score(right, &query_terms);
            let left_score = lesson_score(left, &query_terms);
            right_score.cmp(&left_score)
                .then_with(|| right.confidence.partial_cmp(&left.confidence).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| right.observations.cmp(&left.observations))
        });
        lessons.retain(|lesson| query_terms.is_empty() || lesson_score(lesson, &query_terms) > 0);
        lessons.truncate(limit.max(1));
        Ok(lessons)
    }

    pub fn summary(&self) -> Result<LearningSummary> {
        let lessons = self.load_all_lessons()?;
        Ok(LearningSummary {
            postmortems: self.load_postmortems()?.len(),
            lessons: lessons.len(),
            recurring_failure_patterns: lessons.values().filter(|lesson| lesson.observations >= 2).count(),
        })
    }

    fn load_postmortems(&self) -> Result<Vec<RunPostmortem>> {
        let path = self.root.join("postmortems.jsonl");
        let Ok(file) = fs::File::open(path) else { return Ok(Vec::new()) };
        let mut items = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() { continue; }
            if let Ok(item) = serde_json::from_str(&line) { items.push(item); }
        }
        Ok(items)
    }

    fn load_all_lessons(&self) -> Result<BTreeMap<String, LearnedLesson>> {
        let path = self.root.join("lessons.json");
        let Ok(bytes) = fs::read(path) else { return Ok(BTreeMap::new()) };
        serde_json::from_slice(&bytes).context("failed to parse Yode learning lessons")
    }

    fn save_lessons(&self, lessons: &BTreeMap<String, LearnedLesson>) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let target = self.root.join("lessons.json");
        let temp = self.root.join(format!("lessons-{}.tmp", std::process::id()));
        fs::write(&temp, serde_json::to_vec_pretty(lessons)?)?;
        fs::rename(temp, target)?;
        Ok(())
    }

    fn prune_postmortems(&self, max_items: usize) -> Result<()> {
        let items = self.load_postmortems()?;
        if items.len() <= max_items { return Ok(()) }
        let path = self.root.join("postmortems.jsonl");
        let keep = &items[items.len() - max_items..];
        let mut bytes = Vec::new();
        for item in keep {
            serde_json::to_writer(&mut bytes, item)?;
            bytes.push(b'\n');
        }
        fs::write(path, bytes)?;
        Ok(())
    }
}

fn derive_lessons(postmortem: &RunPostmortem) -> Vec<(String, String, Vec<String>)> {
    let mut lessons = Vec::new();
    for tool in &postmortem.failed_tools {
        let normalized = normalize_key(tool);
        if normalized.is_empty() { continue; }
        lessons.push((
            format!("tool-failure:{normalized}"),
            format!("When `{tool}` fails, diagnose the failure before repeating the same call; prefer a narrower alternative or replan."),
            vec!["tool_failure".to_string(), normalized],
        ));
    }
    if !postmortem.success && postmortem.replans > 0 {
        lessons.push((
            "replan-before-repeat".to_string(),
            "After a failed implementation path, change the hypothesis or execution strategy before retrying the same actions.".to_string(),
            vec!["replan".to_string()],
        ));
    }
    if postmortem.success && !postmortem.verification_evidence.is_empty() {
        lessons.push((
            "verification-before-delivery".to_string(),
            "Use concrete verification evidence before delivery; successful runs should preserve the checks that proved the change.".to_string(),
            vec!["verification".to_string()],
        ));
    }
    for lesson in &postmortem.lessons {
        let key = normalize_key(lesson);
        if !key.is_empty() {
            lessons.push((format!("explicit:{key}"), lesson.clone(), vec!["explicit".to_string()]));
        }
    }
    lessons
}

fn sanitize_postmortem(item: &mut RunPostmortem) {
    item.goal = redact(&item.goal);
    item.summary = redact(&item.summary);
    for value in item.failed_tools.iter_mut()
        .chain(item.verification_evidence.iter_mut())
        .chain(item.model_routes.iter_mut())
        .chain(item.lessons.iter_mut())
    {
        *value = redact(value);
    }
}

fn redact(value: &str) -> String {
    value.split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            if lower.contains("api_key=") || lower.contains("token=") || lower.starts_with("sk-") || lower.starts_with("ghp_") {
                "[REDACTED]".to_string()
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_key(value: &str) -> String {
    value.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("-")
}

fn terms(value: &str) -> HashSet<String> {
    value.to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| part.len() >= 2)
        .map(str::to_string)
        .collect()
}

fn lesson_score(lesson: &LearnedLesson, query_terms: &HashSet<String>) -> u32 {
    let haystack = format!("{} {} {}", lesson.key, lesson.text, lesson.tags.join(" ")).to_ascii_lowercase();
    query_terms.iter().filter(|term| haystack.contains(term.as_str())).count() as u32
}

fn confidence(observations: u32, successful: u32) -> f32 {
    let recurrence = (observations.min(10) as f32) / 10.0;
    let success_signal = if observations == 0 { 0.0 } else { successful as f32 / observations as f32 };
    (0.35 + recurrence * 0.45 + success_signal * 0.20).min(0.98)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_and_ranks_recurring_lessons() {
        let dir = tempfile::tempdir().unwrap();
        let store = LearningStore::for_workspace(dir.path());
        for idx in 0..2 {
            let mut postmortem = RunPostmortem::new(format!("run-{idx}"), "session", "fix tests");
            postmortem.failed_tools = vec!["test_runner".to_string()];
            postmortem.replans = 1;
            store.record(postmortem).unwrap();
        }
        let lessons = store.relevant_lessons("test verification", 5).unwrap();
        assert!(lessons.iter().any(|lesson| lesson.key.contains("test-runner")));
        assert!(store.summary().unwrap().recurring_failure_patterns >= 1);
    }

    #[test]
    fn redacts_common_secret_shapes() {
        assert_eq!(redact("use api_key=secret and sk-abc"), "use [REDACTED] and [REDACTED]");
    }
}
