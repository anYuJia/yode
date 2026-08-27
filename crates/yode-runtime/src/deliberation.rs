use anyhow::{bail, Result};
use async_trait::async_trait;
use futures::future::join_all;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliberationCandidate {
    pub id: String,
    pub label: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JudgeDecision {
    pub winner_id: String,
    pub score: i32,
    pub rationale: String,
    #[serde(default)]
    pub rejected_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BestOfNResult {
    pub candidates: Vec<DeliberationCandidate>,
    pub decision: JudgeDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebateTurn {
    pub round: usize,
    pub agent_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebateResult {
    pub turns: Vec<DebateTurn>,
    pub finalists: Vec<DeliberationCandidate>,
    pub decision: JudgeDecision,
}

#[async_trait]
pub trait DeliberationRunner: Send + Sync {
    async fn generate(&self, label: &str, prompt: &str) -> Result<String>;

    async fn judge(
        &self,
        goal: &str,
        rubric: &str,
        candidates: &[DeliberationCandidate],
    ) -> Result<JudgeDecision>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BestOfNConfig {
    pub n: usize,
    pub labels: Vec<String>,
    pub rubric: String,
}

impl Default for BestOfNConfig {
    fn default() -> Self {
        Self {
            n: 3,
            labels: vec!["candidate-a".into(), "candidate-b".into(), "candidate-c".into()],
            rubric: "Correctness first, then completeness, safety, simplicity, and verification quality.".into(),
        }
    }
}

pub async fn run_best_of_n<R: DeliberationRunner>(
    runner: &R,
    goal: &str,
    config: &BestOfNConfig,
) -> Result<BestOfNResult> {
    let n = config.n.clamp(2, 8);
    let futures = (0..n).map(|index| {
        let label = config
            .labels
            .get(index)
            .cloned()
            .unwrap_or_else(|| format!("candidate-{}", index + 1));
        async move {
            let prompt = format!(
                "Solve the task independently. Do not imitate other candidates. Produce a concrete, verifiable solution.\n\nTask:\n{goal}"
            );
            runner
                .generate(&label, &prompt)
                .await
                .map(|content| DeliberationCandidate {
                    id: format!("candidate-{}", index + 1),
                    label,
                    content,
                })
        }
    });
    let candidates = join_all(futures)
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let decision = runner.judge(goal, &config.rubric, &candidates).await?;
    validate_decision(&decision, &candidates)?;
    Ok(BestOfNResult { candidates, decision })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebateConfig {
    pub agents: usize,
    pub rounds: usize,
    pub rubric: String,
}

impl Default for DebateConfig {
    fn default() -> Self {
        Self {
            agents: 2,
            rounds: 2,
            rubric: "Choose the position that best satisfies the task with correct reasoning, concrete evidence, minimal regressions, and explicit verification.".into(),
        }
    }
}

pub async fn run_debate<R: DeliberationRunner>(
    runner: &R,
    goal: &str,
    config: &DebateConfig,
) -> Result<DebateResult> {
    let agent_count = config.agents.clamp(2, 4);
    let rounds = config.rounds.clamp(1, 4);
    let mut turns = Vec::new();
    let mut latest = vec![String::new(); agent_count];

    for round in 0..rounds {
        let prior_transcript = turns
            .iter()
            .map(|turn: &DebateTurn| format!("{}: {}", turn.agent_id, turn.content))
            .collect::<Vec<_>>()
            .join("\n\n");
        let futures = (0..agent_count).map(|index| {
            let agent_id = format!("debater-{}", index + 1);
            let prior_transcript = prior_transcript.clone();
            async move {
                let instruction = if round == 0 {
                    "Develop an independent solution and identify the highest-risk assumptions."
                } else {
                    "Critique the competing arguments, correct weak assumptions, and present an improved final position."
                };
                let prompt = format!(
                    "You are {agent_id} in round {} of a technical debate. {instruction}\n\nTask:\n{goal}\n\nPrior debate:\n{}",
                    round + 1,
                    if prior_transcript.is_empty() { "(none yet)" } else { &prior_transcript }
                );
                runner.generate(&agent_id, &prompt).await.map(|content| (index, agent_id, content))
            }
        });
        for result in join_all(futures).await {
            let (index, agent_id, content) = result?;
            latest[index] = content.clone();
            turns.push(DebateTurn { round: round + 1, agent_id, content });
        }
    }

    let finalists = latest
        .into_iter()
        .enumerate()
        .map(|(index, content)| DeliberationCandidate {
            id: format!("debater-{}", index + 1),
            label: format!("debater-{}", index + 1),
            content,
        })
        .collect::<Vec<_>>();
    let decision = runner.judge(goal, &config.rubric, &finalists).await?;
    validate_decision(&decision, &finalists)?;
    Ok(DebateResult { turns, finalists, decision })
}

fn validate_decision(decision: &JudgeDecision, candidates: &[DeliberationCandidate]) -> Result<()> {
    if !candidates.iter().any(|candidate| candidate.id == decision.winner_id) {
        bail!("judge selected unknown candidate '{}'", decision.winner_id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockRunner { calls: AtomicUsize }

    #[async_trait]
    impl DeliberationRunner for MockRunner {
        async fn generate(&self, label: &str, prompt: &str) -> Result<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(format!("{label}:{}", prompt.len()))
        }

        async fn judge(&self, _goal: &str, _rubric: &str, candidates: &[DeliberationCandidate]) -> Result<JudgeDecision> {
            Ok(JudgeDecision {
                winner_id: candidates.last().unwrap().id.clone(),
                score: 90,
                rationale: "mock independent judge".into(),
                rejected_reasons: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn best_of_n_generates_and_judges_candidates() {
        let runner = MockRunner { calls: AtomicUsize::new(0) };
        let result = run_best_of_n(&runner, "fix the bug", &BestOfNConfig::default()).await.unwrap();
        assert_eq!(result.candidates.len(), 3);
        assert_eq!(result.decision.winner_id, "candidate-3");
        assert_eq!(runner.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn debate_produces_round_transcript_and_judgment() {
        let runner = MockRunner { calls: AtomicUsize::new(0) };
        let result = run_debate(&runner, "choose an architecture", &DebateConfig::default()).await.unwrap();
        assert_eq!(result.turns.len(), 4);
        assert_eq!(result.finalists.len(), 2);
    }
}
