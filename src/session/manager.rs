//! Session Manager
//!
//! Manages sessions and their associated dictionaries.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use super::codec::{DictEncoder, DictDelta};

/// A session with its dictionary state
#[derive(Debug, Clone)]
pub struct Session {
    /// Session ID
    pub id: String,
    /// Creation timestamp
    pub created_at: u64,
    /// Last accessed timestamp
    pub last_accessed: u64,
    /// Dictionary encoder
    pub encoder: DictEncoder,
    /// Session metadata
    pub metadata: HashMap<String, String>,
}

impl Session {
    pub fn new(id: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id,
            created_at: now,
            last_accessed: now,
            encoder: DictEncoder::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn touch(&mut self) {
        self.last_accessed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Get the current dictionary delta
    pub fn get_dict(&self) -> DictDelta {
        self.encoder.get_delta()
    }
}

/// Manager for multiple sessions
#[derive(Debug, Clone)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    /// Maximum session age in seconds (default: 1 hour)
    max_session_age: u64,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            max_session_age: 3600, // 1 hour
        }
    }

    pub fn with_max_age(max_age_secs: u64) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            max_session_age: max_age_secs,
        }
    }

    /// Create or restore a session
    pub fn open_session(&self, restore_id: Option<&str>) -> Session {
        let mut sessions = self.sessions.lock().unwrap();

        // Try to restore existing session
        if let Some(id) = restore_id {
            if let Some(session) = sessions.get_mut(id) {
                session.touch();
                return session.clone();
            }
        }

        // Create new session
        let id = uuid::Uuid::new_v4().to_string();
        let session = Session::new(id.clone());
        sessions.insert(id, session.clone());

        session
    }

    /// Get a session by ID
    pub fn get_session(&self, id: &str) -> Option<Session> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(id) {
            session.touch();
            Some(session.clone())
        } else {
            None
        }
    }

    /// Update a session's encoder
    pub fn update_session(&self, id: &str, encoder: DictEncoder) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(id) {
            session.encoder = encoder;
            session.touch();
            true
        } else {
            false
        }
    }

    /// Close a session
    pub fn close_session(&self, id: &str) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.remove(id).is_some()
    }

    /// Clean up expired sessions
    pub fn cleanup_expired(&self) -> usize {
        let mut sessions = self.sessions.lock().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| now - s.last_accessed > self.max_session_age)
            .map(|(id, _)| id.clone())
            .collect();

        let count = expired.len();
        for id in expired {
            sessions.remove(&id);
        }
        count
    }

    /// Get session count
    pub fn session_count(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = Session::new("test-session".to_string());
        assert_eq!(session.id, "test-session");
        assert!(session.created_at > 0);
    }

    #[test]
    fn test_session_manager_open() {
        let manager = SessionManager::new();

        let session1 = manager.open_session(None);
        assert!(!session1.id.is_empty());

        let session2 = manager.open_session(Some(&session1.id));
        assert_eq!(session1.id, session2.id);

        assert_eq!(manager.session_count(), 1);
    }

    #[test]
    fn test_session_manager_close() {
        let manager = SessionManager::new();

        let session = manager.open_session(None);
        assert_eq!(manager.session_count(), 1);

        assert!(manager.close_session(&session.id));
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn test_session_encoder_persistence() {
        let manager = SessionManager::new();
        let session = manager.open_session(None);

        // Get the session and update its encoder
        let mut encoder = session.encoder.clone();
        encoder.encode_file("test.rs");

        manager.update_session(&session.id, encoder);

        // Retrieve and verify
        let retrieved = manager.get_session(&session.id).unwrap();
        let delta = retrieved.encoder.get_delta();
        assert_eq!(delta.files.get(&0), Some(&"test.rs".to_string()));
    }
}
