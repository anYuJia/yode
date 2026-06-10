use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A conversation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: Option<String>,
    pub project_root: Option<String>,
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
        if let Some(existing) = self
            .sessions
            .iter_mut()
            .find(|existing| existing.id == session.id)
        {
            *existing = session;
        } else {
            self.sessions.push(session);
        }
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn rename(&mut self, id: &str, name: Option<String>) -> bool {
        if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
            session.name = name;
            session.updated_at = Utc::now();
            true
        } else {
            false
        }
    }

    pub fn touch(&mut self, id: &str) -> bool {
        if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
            session.updated_at = Utc::now();
            true
        } else {
            false
        }
    }

    pub fn delete(&mut self, id: &str) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|session| session.id != id);
        self.sessions.len() != before
    }

    pub fn list(&self) -> &[Session] {
        &self.sessions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, name: Option<&str>, updated_at: DateTime<Utc>) -> Session {
        Session {
            id: id.to_string(),
            name: name.map(str::to_string),
            project_root: None,
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            created_at: updated_at,
            updated_at,
        }
    }

    #[test]
    fn create_replaces_existing_session_id() {
        let now = Utc::now();
        let mut store = SessionStore::new();

        store.create(session("s1", Some("first"), now));
        store.create(session(
            "s1",
            Some("second"),
            now + chrono::Duration::seconds(1),
        ));

        assert_eq!(store.list().len(), 1);
        assert_eq!(store.get("s1").unwrap().name.as_deref(), Some("second"));
    }

    #[test]
    fn rename_touch_and_delete_report_whether_session_exists() {
        let now = Utc::now();
        let mut store = SessionStore::new();
        store.create(session("s1", None, now));

        assert!(store.rename("s1", Some("renamed".to_string())));
        assert_eq!(store.get("s1").unwrap().name.as_deref(), Some("renamed"));
        assert!(store.get("s1").unwrap().updated_at >= now);
        assert!(store.touch("s1"));
        assert!(!store.touch("missing"));
        assert!(store.delete("s1"));
        assert!(!store.delete("s1"));
        assert!(store.get("s1").is_none());
    }
}
