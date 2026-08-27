use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{bail, Result};
use async_trait::async_trait;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use yode_tools::{WorktreeCoordinator, WorktreeFinalizeStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryTarget {
    pub id: String,
    pub path: PathBuf,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub remote: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiRepoStep {
    pub id: String,
    pub repo_id: String,
    pub description: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub mutating: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiRepoPlan {
    pub goal: String,
    pub repositories: Vec<RepositoryTarget>,
    pub steps: Vec<MultiRepoStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiRepoRunRequest {
    pub goal: String,
    pub repository: RepositoryTarget,
    pub step: MultiRepoStep,
    pub workspace: PathBuf,
    pub isolated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiRepoStepResult {
    pub step_id: String,
    pub repo_id: String,
    pub success: bool,
    pub summary: String,
    #[serde(default)]
    pub artifact_path: Option<String>,
    #[serde(default)]
    pub merged_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiRepoExecutionReport {
    pub goal: String,
    pub batches: Vec<Vec<String>>,
    pub results: Vec<MultiRepoStepResult>,
    pub success: bool,
}

#[async_trait]
pub trait MultiRepoRunner: Send + Sync {
    async fn run(&self, request: MultiRepoRunRequest) -> Result<MultiRepoStepResult>;
}

impl MultiRepoPlan {
    pub fn validate(&self) -> Result<()> {
        if self.repositories.is_empty() { bail!("multi-repo plan requires at least one repository"); }
        if self.steps.is_empty() { bail!("multi-repo plan requires at least one step"); }
        let mut repo_ids = HashSet::new();
        let mut canonical_paths = Vec::new();
        for repo in &self.repositories {
            if repo.id.trim().is_empty() || !repo_ids.insert(repo.id.clone()) {
                bail!("repository ids must be non-empty and unique: '{}'", repo.id);
            }
            if !repo.path.is_dir() { bail!("repository '{}' path is not a directory: {}", repo.id, repo.path.display()); }
            let canonical = repo.path.canonicalize().unwrap_or_else(|_| repo.path.clone());
            if canonical_paths.iter().any(|existing: &PathBuf| existing == &canonical || canonical.starts_with(existing) || existing.starts_with(&canonical)) {
                bail!("multi-repo targets must be distinct, non-nested repository roots: {}", canonical.display());
            }
            canonical_paths.push(canonical);
        }
        let mut step_ids = HashSet::new();
        for step in &self.steps {
            if step.id.trim().is_empty() || !step_ids.insert(step.id.clone()) {
                bail!("step ids must be non-empty and unique: '{}'", step.id);
            }
            if !repo_ids.contains(&step.repo_id) { bail!("step '{}' references unknown repository '{}'", step.id, step.repo_id); }
        }
        for step in &self.steps {
            for dep in &step.depends_on {
                if dep == &step.id { bail!("step '{}' cannot depend on itself", step.id); }
                if !step_ids.contains(dep) { bail!("step '{}' depends on unknown step '{}'", step.id, dep); }
            }
        }
        let batches = self.parallel_batches()?;
        let scheduled = batches.iter().flatten().count();
        if scheduled != self.steps.len() { bail!("multi-repo plan contains a dependency cycle"); }
        Ok(())
    }

    pub fn parallel_batches(&self) -> Result<Vec<Vec<String>>> {
        let steps = self.steps.iter().map(|step| (step.id.clone(), step)).collect::<HashMap<_, _>>();
        let mut completed = BTreeSet::new();
        let mut remaining = self.steps.iter().map(|step| step.id.clone()).collect::<BTreeSet<_>>();
        let mut batches = Vec::new();
        while !remaining.is_empty() {
            let ready = remaining
                .iter()
                .filter(|id| {
                    steps.get(*id).is_some_and(|step| step.depends_on.iter().all(|dep| completed.contains(dep)))
                })
                .cloned()
                .collect::<Vec<_>>();
            if ready.is_empty() { break; }
            for id in &ready { remaining.remove(id); completed.insert(id.clone()); }
            batches.push(ready);
        }
        Ok(batches)
    }
}

pub async fn execute_multi_repo_plan<R: MultiRepoRunner>(
    runner: &R,
    plan: &MultiRepoPlan,
) -> Result<MultiRepoExecutionReport> {
    plan.validate()?;
    let batches = plan.parallel_batches()?;
    let repos = plan.repositories.iter().map(|repo| (repo.id.clone(), repo.clone())).collect::<HashMap<_, _>>();
    let steps = plan.steps.iter().map(|step| (step.id.clone(), step.clone())).collect::<HashMap<_, _>>();
    let mut results = BTreeMap::<String, MultiRepoStepResult>::new();

    for batch in &batches {
        if batch.iter().any(|step_id| {
            steps.get(step_id).is_some_and(|step| {
                step.depends_on.iter().any(|dep| results.get(dep).is_some_and(|result| !result.success))
            })
        }) {
            for step_id in batch {
                let step = steps.get(step_id).expect("validated step");
                if step.depends_on.iter().any(|dep| results.get(dep).is_some_and(|result| !result.success)) {
                    results.insert(step_id.clone(), MultiRepoStepResult {
                        step_id: step_id.clone(), repo_id: step.repo_id.clone(), success: false,
                        summary: "Skipped because a dependency failed.".to_string(), artifact_path: None, merged_commit: None,
                    });
                }
            }
        }

        let runnable = batch.iter().filter(|id| !results.contains_key(*id)).cloned().collect::<Vec<_>>();
        let futures = runnable.into_iter().map(|step_id| {
            let step = steps.get(&step_id).expect("validated step").clone();
            let repo = repos.get(&step.repo_id).expect("validated repository").clone();
            async move {
                let lease = if step.mutating {
                    Some(WorktreeCoordinator::allocate(&repo.path, &step.description).await?)
                } else { None };
                let workspace = lease.as_ref().map(|lease| lease.path.clone()).unwrap_or_else(|| repo.path.clone());
                let mut result = runner.run(MultiRepoRunRequest {
                    goal: plan.goal.clone(), repository: repo.clone(), step: step.clone(), workspace,
                    isolated: lease.is_some(),
                }).await?;
                result.step_id = step.id.clone();
                result.repo_id = step.repo_id.clone();

                if let Some(lease) = lease {
                    if result.success {
                        let finalize = WorktreeCoordinator::finalize(
                            &lease,
                            &format!("yode(agent): {}", step.description),
                            true,
                        ).await?;
                        result.merged_commit = finalize.commit.clone();
                        match finalize.status {
                            WorktreeFinalizeStatus::Merged | WorktreeFinalizeStatus::NoChanges => {
                                result.summary = format!("{}\nWorktree: {}", result.summary, finalize.message);
                            }
                            _ => {
                                result.success = false;
                                result.summary = format!("{}\nWorktree merge blocked: {}", result.summary, finalize.message);
                            }
                        }
                    } else {
                        result.summary.push_str(&format!("\nFailed isolated work retained at {} on branch {}.", lease.path.display(), lease.branch));
                    }
                }
                Ok::<_, anyhow::Error>(result)
            }
        });
        for result in join_all(futures).await {
            let result = result?;
            results.insert(result.step_id.clone(), result);
        }
    }

    let ordered = plan.steps.iter().filter_map(|step| results.remove(&step.id)).collect::<Vec<_>>();
    let success = ordered.len() == plan.steps.len() && ordered.iter().all(|result| result.success);
    Ok(MultiRepoExecutionReport { goal: plan.goal.clone(), batches, results: ordered, success })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_cross_repo_dependencies_in_parallel_batches() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let plan = MultiRepoPlan {
            goal: "ship coordinated change".into(),
            repositories: vec![
                RepositoryTarget { id:"api".into(), path:a, branch:None, remote:None },
                RepositoryTarget { id:"ui".into(), path:b, branch:None, remote:None },
            ],
            steps: vec![
                MultiRepoStep { id:"api-change".into(), repo_id:"api".into(), description:"change api".into(), depends_on:vec![], mutating:true },
                MultiRepoStep { id:"ui-change".into(), repo_id:"ui".into(), description:"change ui".into(), depends_on:vec![], mutating:true },
                MultiRepoStep { id:"integration".into(), repo_id:"ui".into(), description:"verify integration".into(), depends_on:vec!["api-change".into(),"ui-change".into()], mutating:false },
            ],
        };
        plan.validate().unwrap();
        assert_eq!(plan.parallel_batches().unwrap(), vec![vec!["api-change".to_string(),"ui-change".to_string()], vec!["integration".to_string()]]);
    }

    #[test]
    fn rejects_nested_repo_roots() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let plan = MultiRepoPlan {
            goal:"bad".into(), repositories:vec![
                RepositoryTarget { id:"a".into(), path:root.path().to_path_buf(), branch:None, remote:None },
                RepositoryTarget { id:"b".into(), path:nested, branch:None, remote:None },
            ],
            steps:vec![MultiRepoStep { id:"x".into(), repo_id:"a".into(), description:"x".into(), depends_on:vec![], mutating:false }],
        };
        assert!(plan.validate().is_err());
    }
}
