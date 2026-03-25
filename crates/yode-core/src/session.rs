use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A conversation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: Option<String>,
    pub provider: String,
    pub model: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Simple in-memory session store (SQLite persistence in Phase 2).
#[derive(Debug, Default)]
pub struct SessionStore {
    sessions: Vec<Session>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, session: Session) {
        self.sessions.push(session);
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn list(&self) -> &[Session] {
        &self.sessions
    }
}
