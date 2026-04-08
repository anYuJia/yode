use std::collections::HashMap;
use tokio::sync::mpsc;

/// A scheduled cron job (session-scoped, not persisted).
#[derive(Debug, Clone)]
pub struct CronJob {
    pub id: String,
    pub cron_expr: String,
    pub prompt: String,
    pub recurring: bool,
    pub next_fire: chrono::DateTime<chrono::Local>,
    pub created_at: std::time::Instant,
}

/// Manages session-scoped cron jobs.
pub struct CronManager {
    jobs: HashMap<String, CronJob>,
    next_id: u64,
    prompt_tx: mpsc::UnboundedSender<String>,
}

impl CronManager {
    pub fn new(prompt_tx: mpsc::UnboundedSender<String>) -> Self {
        Self {
            jobs: HashMap::new(),
            next_id: 1,
            prompt_tx,
        }
    }

    /// Create a new cron job. Returns the job ID.
    pub fn create(
        &mut self,
        cron_expr: String,
        prompt: String,
        recurring: bool,
    ) -> anyhow::Result<String> {
        let schedule = Self::parse_cron(&cron_expr)?;
        let id = self.next_id.to_string();
        self.next_id += 1;

        let job = CronJob {
            id: id.clone(),
            cron_expr,
            prompt,
            recurring,
            next_fire: schedule,
            created_at: std::time::Instant::now(),
        };
        self.jobs.insert(id.clone(), job);
        Ok(id)
    }

    /// Delete a cron job by ID.
    pub fn delete(&mut self, id: &str) -> bool {
        self.jobs.remove(id).is_some()
    }

    /// List all cron jobs.
    pub fn list(&self) -> Vec<&CronJob> {
        self.jobs.values().collect()
    }

    /// Check and fire any due jobs. Called periodically from the main loop.
    pub fn tick(&mut self) -> Vec<String> {
        let now = chrono::Local::now();
        let mut fired = Vec::new();
        let mut to_remove = Vec::new();

        for (id, job) in &self.jobs {
            // Auto-expire recurring jobs after 3 days
            if job.created_at.elapsed() > std::time::Duration::from_secs(3 * 24 * 60 * 60) {
                to_remove.push(id.clone());
                continue;
            }

            if now >= job.next_fire {
                let _ = self.prompt_tx.send(job.prompt.clone());
                fired.push(id.clone());
                if !job.recurring {
                    to_remove.push(id.clone());
                }
            }
        }

        // Update next_fire for recurring jobs that fired
        for id in &fired {
            if let Some(job) = self.jobs.get_mut(id) {
                if job.recurring {
                    if let Ok(next) = Self::parse_cron(&job.cron_expr) {
                        job.next_fire = next;
                    }
                }
            }
        }

        for id in to_remove {
            self.jobs.remove(&id);
        }

        fired
    }

    /// Parse a 5-field cron expression and return the next fire time.
    fn parse_cron(expr: &str) -> anyhow::Result<chrono::DateTime<chrono::Local>> {
        let schedule: cron::Schedule = format!("0 {}", expr)
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", expr, e))?;
        schedule
            .upcoming(chrono::Utc)
            .next()
            .map(|dt| dt.with_timezone(&chrono::Local))
            .ok_or_else(|| anyhow::anyhow!("No upcoming fire time for cron expression '{}'", expr))
    }
}
