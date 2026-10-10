//! Durable CMA progress in the existing referee aggregate, fenced by the job lease.
use super::worker::Worker;
use async_trait::async_trait;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use wishpool_core::{
    app::App,
    model::{AgentStep, Caller, Submission, SubmissionStatus},
    ports::{JobLease, LeasedJob},
};
use wishpool_review::{ReviewError, ReviewResult, cma::RunStore};

pub(super) struct AgentStore {
    app: Arc<App>,
    jobs: Arc<dyn JobLease>,
    job: LeasedJob,
    caller: Caller,
    key: String,
    version: u32,
    claims_revision: u64,
}
fn fence() -> ReviewError {
    ReviewError::Transport("CMA persistence fence lost".into())
}
impl Worker {
    pub(super) async fn cleanup_agents(
        &self,
        job: &LeasedJob,
    ) -> Result<(), super::worker::Failure> {
        let Some(client) = self.advisor.as_ref().and_then(|a| a.managed_client()) else {
            return Ok(());
        };
        let paper = self
            .app
            .submission(&self.auditor_account, &job.submission)
            .await?;
        for (key, step) in self
            .app
            .agent_steps(&self.auditor_account, &job.submission)
            .await?
        {
            let state: wishpool_review::cma::RunState = match serde_json::from_value(step.progress)
            {
                Ok(s) => s,
                Err(_) => continue,
            };
            if state.deleted {
                continue;
            }
            let superseded = paper
                .current_version()
                .is_none_or(|v| v.number != step.version)
                || paper.claims_revision != step.claims_revision
                || matches!(
                    paper.status,
                    SubmissionStatus::Draft | SubmissionStatus::Withdrawn
                );
            let expired = chrono::Utc::now().timestamp().max(0) as u64 >= state.deadline;
            if !superseded && !expired && state.answer.is_none() && state.failure.is_none() {
                continue;
            }
            let store = AgentStore {
                app: self.app.clone(),
                jobs: self.jobs.clone(),
                job: job.clone(),
                caller: self.auditor_account.clone(),
                key,
                version: step.version,
                claims_revision: step.claims_revision,
            };
            if let Err(error) = client.cancel(&store).await {
                tracing::warn!(%error,"CMA cleanup will be retried on the next paper job");
            }
        }
        Ok(())
    }

    pub(super) fn agent_store(
        &self,
        job: &LeasedJob,
        paper: &Submission,
        step: &str,
    ) -> AgentStore {
        let version = paper.current_version().map_or(0, |v| v.number);
        let identity = format!("{}:v{version}:c{}:{step}", paper.id, paper.claims_revision);
        AgentStore {
            app: self.app.clone(),
            jobs: self.jobs.clone(),
            job: job.clone(),
            caller: self.auditor_account.clone(),
            key: format!("wishpool:{:x}", Sha256::digest(identity.as_bytes())),
            version,
            claims_revision: paper.claims_revision,
        }
    }
}
#[async_trait]
impl RunStore for AgentStore {
    fn key(&self) -> &str {
        &self.key
    }
    async fn active(&self) -> ReviewResult<bool> {
        if !self.jobs.renew(&self.job).await.map_err(|_| fence())? {
            return Err(fence());
        }
        let paper = self
            .app
            .submission(&self.caller, &self.job.submission)
            .await
            .map_err(|_| fence())?;
        Ok(paper
            .current_version()
            .is_some_and(|v| v.number == self.version)
            && paper.claims_revision == self.claims_revision
            && !matches!(
                paper.status,
                SubmissionStatus::Draft | SubmissionStatus::Withdrawn
            ))
    }
    async fn load(&self) -> ReviewResult<Option<Value>> {
        if !self.jobs.renew(&self.job).await.map_err(|_| fence())? {
            return Err(fence());
        }
        Ok(self
            .app
            .agent_step(&self.caller, &self.job.submission, &self.key)
            .await
            .map_err(|_| fence())?
            .map(|s| s.progress))
    }
    async fn save(&self, progress: Value) -> ReviewResult<()> {
        if !self.jobs.renew(&self.job).await.map_err(|_| fence())? {
            return Err(fence());
        }
        self.app
            .save_agent_step(
                &self.caller,
                &self.job.submission,
                &self.key,
                AgentStep {
                    version: self.version,
                    claims_revision: self.claims_revision,
                    progress,
                },
            )
            .await
            .map_err(|_| fence())
    }
}
