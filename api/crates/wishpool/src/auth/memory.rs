use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Mutex;
use wishpool_core::CoreResult;

use super::{AuthStore, LoginAttempt, SessionRecord};

/// Process-memory sessions for the `memory` storage mode.
#[derive(Default)]
pub struct MemoryAuthStore {
    sessions: Mutex<HashMap<String, SessionRecord>>,
    attempts: Mutex<HashMap<String, LoginAttempt>>,
}

#[async_trait]
impl AuthStore for MemoryAuthStore {
    async fn create_session(&self, digest: &str, session: &SessionRecord) -> CoreResult<()> {
        self.sessions
            .lock()
            .await
            .insert(digest.to_owned(), session.clone());
        Ok(())
    }

    async fn session(&self, digest: &str) -> CoreResult<Option<SessionRecord>> {
        let now = Utc::now();
        Ok(self
            .sessions
            .lock()
            .await
            .get(digest)
            .filter(|s| s.expires_at > now)
            .cloned())
    }

    async fn delete_session(&self, digest: &str) -> CoreResult<()> {
        self.sessions.lock().await.remove(digest);
        Ok(())
    }

    async fn put_attempt(&self, state_digest: &str, attempt: &LoginAttempt) -> CoreResult<()> {
        self.attempts
            .lock()
            .await
            .insert(state_digest.to_owned(), attempt.clone());
        Ok(())
    }

    async fn take_attempt(&self, state_digest: &str) -> CoreResult<Option<LoginAttempt>> {
        Ok(self.attempts.lock().await.remove(state_digest))
    }
}
