//! Machine work on papers: a durable job queue and the worker that drains
//! it (compiling sources, drafting literature leads, proposing escape
//! judgements).
//!
//! Jobs are recorded in the database before any work starts. A worker leases
//! one job at a time; a lease that outlives its holder expires and the job is
//! retried, so a restart never loses or duplicates admitted work. The worker
//! files every result through Layer 2 as the reviewer service account, under
//! the same rules as any other reviewer.

mod agent_store;
pub(crate) mod mapping;
mod open_problems;
mod referee;
mod sources;
mod worker;

use async_trait::async_trait;
use wishpool_core::{CoreResult, ids::SubmissionId, model::Stage, ports::JobKind};

pub use worker::{Worker, run_reconciler};

pub const MAX_ATTEMPTS: u32 = 3;

// Keep the old module paths available to the binary and its tests while the
// port itself lives with the other Layer 2 ports.
pub use wishpool_core::ports::{JobLease, LeasedJob};

/// The stored form of a job kind: `compile` or `stage:<stage>`.
pub fn kind_key(kind: JobKind) -> String {
    match kind {
        JobKind::Compile => "compile".to_owned(),
        JobKind::Referee => "referee".to_owned(),
        JobKind::OpenProblems => "open_problems".to_owned(),
        JobKind::VerifyAttempt => "verify_attempt".to_owned(),
        JobKind::LeanStatement => "lean_statement".to_owned(),
        JobKind::Stage(stage) => format!(
            "stage:{}",
            serde_json::to_value(stage)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default()
        ),
    }
}

pub fn parse_kind_key(key: &str) -> Option<JobKind> {
    if key == "open_problems" {
        return Some(JobKind::OpenProblems);
    }
    if key == "verify_attempt" {
        return Some(JobKind::VerifyAttempt);
    }
    if key == "lean_statement" {
        return Some(JobKind::LeanStatement);
    }
    if key == "referee" {
        return Some(JobKind::Referee);
    }
    if key == "compile" {
        return Some(JobKind::Compile);
    }
    let stage: Stage = serde_json::from_value(serde_json::Value::String(
        key.strip_prefix("stage:")?.to_owned(),
    ))
    .ok()?;
    Some(JobKind::Stage(stage))
}

/// Memory-mode job source over the core memory stores. Retains live and waiting
/// jobs so renewal, expiry and stale-token fencing match the durable store.
pub struct MemoryJobs {
    pub stores: std::sync::Arc<wishpool_core::memory::MemoryStores>,
    jobs: tokio::sync::Mutex<std::collections::BTreeMap<String, MemoryJob>>,
}

const MEMORY_LEASE: std::time::Duration = std::time::Duration::from_secs(30 * 60);

struct MemoryJob {
    job: LeasedJob,
    state: MemoryJobState,
}

enum MemoryJobState {
    Queued(tokio::time::Instant),
    Leased(tokio::time::Instant),
    Failed,
}

impl MemoryJob {
    fn owns(&self, job: &LeasedJob) -> bool {
        self.job.lease == job.lease
            && matches!(self.state, MemoryJobState::Leased(until) if until > tokio::time::Instant::now())
    }
}

fn memory_job_key(submission: &SubmissionId, kind: JobKind) -> String {
    format!("{submission}:{}", kind_key(kind))
}

impl MemoryJobs {
    pub fn new(stores: std::sync::Arc<wishpool_core::memory::MemoryStores>) -> Self {
        Self {
            stores,
            jobs: Default::default(),
        }
    }

    #[cfg(test)]
    async fn current(&self, submission: &SubmissionId, kind: JobKind) -> Option<LeasedJob> {
        self.jobs
            .lock()
            .await
            .get(&memory_job_key(submission, kind))
            .filter(|entry| entry.owns(&entry.job))
            .map(|entry| entry.job.clone())
    }
}

#[async_trait]
impl JobLease for MemoryJobs {
    async fn claim(&self) -> CoreResult<Option<LeasedJob>> {
        let now = tokio::time::Instant::now();
        let mut jobs = self.jobs.lock().await;
        // Admission still goes through the Layer 2 queue. As in Mongo, an
        // enqueue preserves in-flight work and resets other jobs for replay.
        while let Some(job) = self.stores.take_job().await {
            let key = memory_job_key(&job.submission, job.kind);
            if jobs
                .get(&key)
                .is_some_and(|entry| matches!(entry.state, MemoryJobState::Leased(_)))
            {
                continue;
            }
            jobs.insert(
                key,
                MemoryJob {
                    job: LeasedJob {
                        submission: job.submission,
                        kind: job.kind,
                        attempts: 0,
                        lease: String::new(),
                    },
                    state: MemoryJobState::Queued(now),
                },
            );
        }
        let available = jobs
            .iter()
            .filter_map(|(key, entry)| match entry.state {
                MemoryJobState::Queued(at) | MemoryJobState::Leased(at) if at <= now => {
                    Some((key.clone(), at))
                }
                _ => None,
            })
            .min_by_key(|(_, at)| *at)
            .map(|(key, _)| key);
        let Some(key) = available else {
            return Ok(None);
        };
        let entry = jobs.get_mut(&key).expect("available job exists");
        entry.job.attempts += 1;
        entry.job.lease = crate::auth::random_token();
        entry.state = MemoryJobState::Leased(now + MEMORY_LEASE);
        Ok(Some(entry.job.clone()))
    }

    async fn renew(&self, job: &LeasedJob) -> CoreResult<bool> {
        let mut jobs = self.jobs.lock().await;
        let Some(entry) = jobs.get_mut(&memory_job_key(&job.submission, job.kind)) else {
            return Ok(false);
        };
        if !entry.owns(job) {
            return Ok(false);
        }
        entry.state = MemoryJobState::Leased(tokio::time::Instant::now() + MEMORY_LEASE);
        Ok(true)
    }

    async fn complete(&self, job: &LeasedJob) -> CoreResult<()> {
        let mut jobs = self.jobs.lock().await;
        let key = memory_job_key(&job.submission, job.kind);
        if jobs.get(&key).is_some_and(|entry| entry.owns(job)) {
            jobs.remove(&key);
        }
        Ok(())
    }

    async fn defer(&self, job: &LeasedJob, delay: std::time::Duration) -> CoreResult<()> {
        let mut jobs = self.jobs.lock().await;
        if let Some(entry) = jobs
            .get_mut(&memory_job_key(&job.submission, job.kind))
            .filter(|entry| entry.owns(job))
        {
            entry.state = MemoryJobState::Queued(tokio::time::Instant::now() + delay);
            entry.job.attempts -= 1;
            entry.job.lease.clear();
        }
        Ok(())
    }

    async fn retry(&self, job: &LeasedJob, error: &str) -> CoreResult<()> {
        let mut jobs = self.jobs.lock().await;
        if let Some(entry) = jobs
            .get_mut(&memory_job_key(&job.submission, job.kind))
            .filter(|entry| entry.owns(job))
        {
            entry.state = if job.attempts >= MAX_ATTEMPTS {
                MemoryJobState::Failed
            } else {
                MemoryJobState::Queued(
                    tokio::time::Instant::now()
                        + std::time::Duration::from_secs(30 * u64::from(job.attempts)),
                )
            };
            entry.job.lease.clear();
            tracing::warn!(submission = %job.submission, kind = kind_key(job.kind), error, "memory-mode job failed");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_keys_round_trip() {
        for kind in [
            JobKind::Compile,
            JobKind::Referee,
            JobKind::Stage(Stage::Literature),
            JobKind::Stage(Stage::Escape),
        ] {
            assert_eq!(parse_kind_key(&kind_key(kind)), Some(kind));
        }
        assert_eq!(
            kind_key(JobKind::Stage(Stage::Literature)),
            "stage:literature"
        );
        assert_eq!(parse_kind_key("stage:nonsense"), None);
    }
}

#[cfg(test)]
mod defer_tests {
    use super::*;
    use std::{sync::Arc, time::Duration};
    use wishpool_core::{memory::MemoryStores, ports::ReviewQueue};

    #[tokio::test(start_paused = true)]
    async fn memory_deferral_preserves_attempts_and_releases_worker() {
        let stores = Arc::new(MemoryStores::default());
        let jobs = MemoryJobs::new(stores.clone());
        stores
            .enqueue(&"paper".into(), JobKind::Referee)
            .await
            .unwrap();
        let job = jobs.claim().await.unwrap().unwrap();
        jobs.defer(&job, Duration::from_secs(60)).await.unwrap();
        assert!(jobs.claim().await.unwrap().is_none());
        stores
            .enqueue(&"other".into(), JobKind::Compile)
            .await
            .unwrap();
        assert_eq!(
            jobs.claim().await.unwrap().unwrap().submission.as_str(),
            "other"
        );
        tokio::time::advance(Duration::from_secs(60)).await;
        assert_eq!(jobs.claim().await.unwrap().unwrap().attempts, job.attempts);
    }

    #[tokio::test(start_paused = true)]
    async fn memory_renewal_extends_lease_and_fences_expired_workers() {
        let stores = Arc::new(MemoryStores::default());
        let jobs = MemoryJobs::new(stores.clone());
        stores
            .enqueue(&"paper".into(), JobKind::Referee)
            .await
            .unwrap();
        let first = jobs.claim().await.unwrap().unwrap();
        tokio::time::advance(Duration::from_secs(25 * 60)).await;
        assert!(jobs.renew(&first).await.unwrap());
        tokio::time::advance(Duration::from_secs(10 * 60)).await;
        assert!(jobs.claim().await.unwrap().is_none());
        tokio::time::advance(Duration::from_secs(21 * 60)).await;
        assert!(
            !jobs.renew(&first).await.unwrap(),
            "an expired token cannot revive its lease"
        );
        let second = jobs.claim().await.unwrap().unwrap();
        assert_eq!(second.attempts, 2);
        assert_ne!(first.lease, second.lease);
        assert!(!jobs.renew(&first).await.unwrap());
        jobs.complete(&first).await.unwrap();
        jobs.defer(&first, Duration::ZERO).await.unwrap();
        jobs.retry(&first, "stale").await.unwrap();
        assert!(jobs.renew(&second).await.unwrap());
        assert!(jobs.claim().await.unwrap().is_none());
        jobs.complete(&second).await.unwrap();
        assert!(!jobs.renew(&second).await.unwrap());
    }

    #[tokio::test(start_paused = true)]
    async fn memory_retries_keep_existing_job_attempt_limit() {
        let stores = Arc::new(MemoryStores::default());
        let jobs = MemoryJobs::new(stores.clone());
        stores
            .enqueue(&"paper".into(), JobKind::Referee)
            .await
            .unwrap();
        for attempt in 1..=MAX_ATTEMPTS {
            let job = jobs.claim().await.unwrap().unwrap();
            assert_eq!(job.attempts, attempt);
            jobs.retry(&job, "transport timeout").await.unwrap();
            assert!(!jobs.renew(&job).await.unwrap());
            assert!(jobs.claim().await.unwrap().is_none());
            tokio::time::advance(Duration::from_secs(30 * u64::from(attempt))).await;
        }
        assert!(jobs.claim().await.unwrap().is_none());
    }
}
