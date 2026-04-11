use rusqlite::params;

use super::*;
use crate::session::Session;

impl Database {
    pub fn create_session(&self, session: &Session) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (id, name, provider, model, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session.id,
                session.name,
                session.provider,
                session.model,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn touch_session(&self, session_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![now, session_id],
        )?;
        Ok(())
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, provider, model, created_at, updated_at FROM sessions WHERE id = ?1",
        )?;

        let mut rows = stmt.query(params![session_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Session {
                id: row.get(0)?,
                name: row.get(1)?,
                provider: row.get(2)?,
                model: row.get(3)?,
                created_at: parse_rfc3339_or_now(row.get::<_, String>(4)?),
                updated_at: parse_rfc3339_or_now(row.get::<_, String>(5)?),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_sessions(&self, limit: usize) -> Result<Vec<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, provider, model, created_at, updated_at FROM sessions ORDER BY updated_at DESC LIMIT ?1",
        )?;

        let sessions = stmt
            .query_map(params![limit as i64], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    provider: row.get(2)?,
                    model: row.get(3)?,
                    created_at: parse_rfc3339_or_now(row.get::<_, String>(4).unwrap_or_default()),
                    updated_at: parse_rfc3339_or_now(row.get::<_, String>(5).unwrap_or_default()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(sessions)
    }

    pub fn list_sessions_with_artifacts(&self, limit: usize) -> Result<Vec<SessionListEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT
                s.id, s.name, s.provider, s.model, s.created_at, s.updated_at,
                a.last_compaction_mode, a.last_compaction_at, a.last_compaction_summary_excerpt,
                a.last_compaction_session_memory_path, a.last_compaction_transcript_path,
                a.last_session_memory_update_at, a.last_session_memory_update_path,
                a.last_session_memory_generated_summary
             FROM sessions s
             LEFT JOIN session_artifacts a ON a.session_id = s.id
             ORDER BY s.updated_at DESC
             LIMIT ?1",
        )?;

        let entries = stmt
            .query_map(params![limit as i64], |row| {
                Ok(SessionListEntry {
                    session: Session {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        provider: row.get(2)?,
                        model: row.get(3)?,
                        created_at: parse_rfc3339_or_now(
                            row.get::<_, String>(4).unwrap_or_default(),
                        ),
                        updated_at: parse_rfc3339_or_now(
                            row.get::<_, String>(5).unwrap_or_default(),
                        ),
                    },
                    artifacts: SessionArtifacts {
                        last_compaction_mode: row.get(6)?,
                        last_compaction_at: row.get(7)?,
                        last_compaction_summary_excerpt: row.get(8)?,
                        last_compaction_session_memory_path: row.get(9)?,
                        last_compaction_transcript_path: row.get(10)?,
                        last_session_memory_update_at: row.get(11)?,
                        last_session_memory_update_path: row.get(12)?,
                        last_session_memory_generated_summary: row
                            .get::<_, Option<i64>>(13)?
                            .unwrap_or(0)
                            != 0,
                    },
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    pub fn upsert_session_artifacts(
        &self,
        session_id: &str,
        artifacts: &SessionArtifacts,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO session_artifacts (
                session_id,
                last_compaction_mode,
                last_compaction_at,
                last_compaction_summary_excerpt,
                last_compaction_session_memory_path,
                last_compaction_transcript_path,
                last_session_memory_update_at,
                last_session_memory_update_path,
                last_session_memory_generated_summary,
                updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(session_id) DO UPDATE SET
                last_compaction_mode = excluded.last_compaction_mode,
                last_compaction_at = excluded.last_compaction_at,
                last_compaction_summary_excerpt = excluded.last_compaction_summary_excerpt,
                last_compaction_session_memory_path = excluded.last_compaction_session_memory_path,
                last_compaction_transcript_path = excluded.last_compaction_transcript_path,
                last_session_memory_update_at = excluded.last_session_memory_update_at,
                last_session_memory_update_path = excluded.last_session_memory_update_path,
                last_session_memory_generated_summary = excluded.last_session_memory_generated_summary,
                updated_at = excluded.updated_at",
            params![
                session_id,
                artifacts.last_compaction_mode,
                artifacts.last_compaction_at,
                artifacts.last_compaction_summary_excerpt,
                artifacts.last_compaction_session_memory_path,
                artifacts.last_compaction_transcript_path,
                artifacts.last_session_memory_update_at,
                artifacts.last_session_memory_update_path,
                if artifacts.last_session_memory_generated_summary {
                    1
                } else {
                    0
                },
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Update session name
    pub fn update_session_name(&self, session_id: &str, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, Utc::now().to_rfc3339(), session_id],
        )?;
        Ok(())
    }
}
