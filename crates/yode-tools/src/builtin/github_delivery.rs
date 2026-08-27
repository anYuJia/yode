use std::path::Path;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::process::Command;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct GitHubIssueDeliveryTool;

#[derive(Debug, Clone, Deserialize)]
struct IssueView {
    number: u64,
    title: String,
    body: String,
    url: String,
    state: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckView {
    name: String,
    state: Option<String>,
    bucket: Option<String>,
    link: Option<String>,
}

#[async_trait]
impl Tool for GitHubIssueDeliveryTool {
    fn name(&self) -> &str {
        "github_issue_delivery"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["issue_to_pr".to_string(), "github_pr_delivery".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        "GitHub Issue → PR"
    }

    fn activity_description(&self, params: &Value) -> String {
        let issue = params
            .get("issue_number")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        format!("Delivering GitHub issue #{issue} as a PR")
    }

    fn description(&self) -> &str {
        "Complete the GitHub delivery loop for already-implemented work: verify gh authentication and repository state, read an issue, push the current head branch, create a pull request linked with Fixes #N, then return live PR check status. Never fabricates a PR or CI result."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "issue_number": {"type":"integer","minimum":1},
                "base_branch": {"type":"string","default":"main"},
                "head_branch": {"type":"string","description":"Branch containing the completed work; defaults to current branch."},
                "title": {"type":"string","description":"Optional PR title; defaults to issue title."},
                "body": {"type":"string","description":"Optional PR body. `Fixes #N` is always appended."},
                "draft": {"type":"boolean","default":false}
            },
            "required": ["issue_number"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let issue_number = params
            .get("issue_number")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow::anyhow!("issue_number is required"))?;
        let base = params
            .get("base_branch")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .trim();
        validate_ref(base)?;
        let cwd = ctx.working_dir.as_deref().unwrap_or_else(|| Path::new("."));

        if let Err(error) = require_gh(cwd).await {
            return Ok(ToolResult::error_typed(
                format!("GitHub delivery unavailable: {error:#}"),
                ToolErrorType::Execution,
                true,
                Some("Install GitHub CLI (`gh`) and run `gh auth login`, then retry.".to_string()),
            ));
        }

        let issue: IssueView = gh_json(
            cwd,
            &[
                "issue",
                "view",
                &issue_number.to_string(),
                "--json",
                "number,title,body,url,state",
            ],
        )
        .await
        .with_context(|| format!("failed to read GitHub issue #{issue_number}"))?;
        if !issue.state.eq_ignore_ascii_case("open") {
            return Ok(ToolResult::error_typed(
                format!(
                    "Issue #{} is not open (state: {}).",
                    issue.number, issue.state
                ),
                ToolErrorType::Validation,
                false,
                None,
            ));
        }

        let current_branch = git_text(cwd, &["branch", "--show-current"]).await?;
        let head = params
            .get("head_branch")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| current_branch.trim());
        validate_ref(head)?;
        if head == base {
            return Ok(ToolResult::error_typed(
                format!("Refusing GitHub delivery from base branch '{base}'. Create a feature/agent branch first."),
                ToolErrorType::Validation,
                false,
                Some("Use an isolated worktree branch for issue implementation, then deliver it.".to_string()),
            ));
        }
        if current_branch.trim() != head {
            return Ok(ToolResult::error_typed(
                format!(
                    "Current branch '{}' does not match requested head '{}'.",
                    current_branch.trim(),
                    head
                ),
                ToolErrorType::Validation,
                false,
                None,
            ));
        }

        let dirty = git_text(cwd, &["status", "--porcelain=v1"]).await?;
        if !dirty.trim().is_empty() {
            return Ok(ToolResult::error_typed(
                "Working tree is dirty; refusing to push an incomplete delivery.".to_string(),
                ToolErrorType::Validation,
                true,
                Some("Commit or revert local changes after verification, then retry.".to_string()),
            ));
        }
        let ahead = git_text(cwd, &["rev-list", "--count", &format!("{base}..HEAD")]).await?;
        if ahead.trim().parse::<u64>().unwrap_or(0) == 0 {
            return Ok(ToolResult::error_typed(
                format!("Branch '{head}' has no commits ahead of '{base}'."),
                ToolErrorType::Validation,
                false,
                None,
            ));
        }

        git_status(cwd, &["push", "-u", "origin", head])
            .await
            .context("failed to push delivery branch")?;

        let title = params
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(issue.title.trim());
        let supplied_body = params
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let body = if supplied_body.is_empty() {
            format!(
                "Implements issue #{}: {}\n\nIssue: {}\n\nFixes #{}",
                issue.number, issue.title, issue.url, issue.number
            )
        } else {
            format!("{}\n\nFixes #{}", supplied_body, issue.number)
        };
        let mut args = vec![
            "pr", "create", "--base", base, "--head", head, "--title", title, "--body", &body,
        ];
        if params
            .get("draft")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            args.push("--draft");
        }
        let pr_url = gh_text(cwd, &args)
            .await
            .context("failed to create pull request")?;
        let pr_url = pr_url.trim().to_string();
        if pr_url.is_empty() {
            bail!("gh pr create succeeded without returning a PR URL");
        }

        let checks: Vec<CheckView> = gh_json(
            cwd,
            &["pr", "checks", &pr_url, "--json", "name,state,bucket,link"],
        )
        .await
        .unwrap_or_default();
        let failing = checks
            .iter()
            .filter(|check| matches!(check.bucket.as_deref(), Some("fail") | Some("cancel")))
            .count();
        let passing = checks
            .iter()
            .filter(|check| check.bucket.as_deref() == Some("pass"))
            .count();
        let pending = checks.len().saturating_sub(failing + passing);

        Ok(ToolResult::success_with_metadata(
            format!(
                "Created PR for issue #{}: {}\nCI checks: {} passing, {} pending, {} failing.",
                issue.number, pr_url, passing, pending, failing
            ),
            json!({
                "issue": {"number":issue.number,"title":issue.title,"url":issue.url,"body_excerpt":issue.body.chars().take(240).collect::<String>()},
                "base_branch": base,
                "head_branch": head,
                "pr_url": pr_url,
                "checks": checks,
                "checks_summary": {"passing":passing,"pending":pending,"failing":failing},
                "delivery_complete": failing == 0 && pending == 0 && !checks.is_empty()
            }),
        ))
    }
}

async fn require_gh(cwd: &Path) -> Result<()> {
    let version = Command::new("gh")
        .arg("--version")
        .current_dir(cwd)
        .output()
        .await
        .context("GitHub CLI executable not found")?;
    if !version.status.success() {
        bail!("`gh --version` failed");
    }
    let auth = Command::new("gh")
        .args(["auth", "status"])
        .current_dir(cwd)
        .output()
        .await?;
    if !auth.status.success() {
        bail!(
            "GitHub CLI is not authenticated: {}",
            String::from_utf8_lossy(&auth.stderr).trim()
        );
    }
    Ok(())
}

async fn gh_text(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("gh")
        .args(args)
        .current_dir(cwd)
        .output()
        .await?;
    if !output.status.success() {
        bail!(
            "gh {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn gh_json<T: for<'de> Deserialize<'de>>(cwd: &Path, args: &[&str]) -> Result<T> {
    let text = gh_text(cwd, args).await?;
    serde_json::from_str(&text).context("failed to decode gh JSON output")
}

async fn git_text(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn git_status(cwd: &Path, args: &[&str]) -> Result<()> {
    git_text(cwd, args).await.map(|_| ())
}

fn validate_ref(value: &str) -> Result<()> {
    if value.is_empty()
        || value.starts_with('-')
        || value.contains("..")
        || value.contains("@{")
        || value.chars().any(|ch| {
            ch.is_control()
                || ch.is_whitespace()
                || matches!(ch, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
    {
        bail!("invalid git ref '{value}'");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_refs() {
        assert!(validate_ref("feature/agent-work").is_ok());
        assert!(validate_ref("--upload-pack=evil").is_err());
        assert!(validate_ref("main..other").is_err());
        assert!(validate_ref("bad branch").is_err());
    }
}
