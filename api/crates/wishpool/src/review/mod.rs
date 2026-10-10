//! Machine work on papers: a durable job queue and the worker that drains
//! it (compiling sources, drafting literature leads, proposing escape
//! judgements).
//!
//! Jobs are recorded in the database before any work starts. A worker leases
//! one job at a time; a lease that outlives its holder expires and the job is
//! retried, so a restart never loses or duplicates admitted work. The worker
//! files every result through Layer 2 as the reviewer service account, under
//! the same rules as any other reviewer.

pub(crate) mod mapping;
mod referee;
mod worker;

use async_trait::async_trait;
use wishpool_core::{
    CoreResult,
    ids::SubmissionId,
    model::{Stage, normalise_doi},
    ports::JobKind,
};

pub use worker::{Worker, run_reconciler};

/// Candidates kept per statement across its queries.
pub const CANDIDATES_PER_STATEMENT: usize = 10;
const CANDIDATES_PER_QUERY: usize = 6;

/// The paper a literature search runs for; its own entries are excluded.
pub struct SearchedPaper<'a> {
    pub title: &'a str,
    pub abstract_text: &'a str,
    pub doi: Option<&'a str>,
}

/// Search OpenAlex for one statement and relate what it returns: the model
/// writes the queries, the search supplies the works, the model relates
/// only those works. Returns the leads and the summed usage.
pub(crate) async fn search_statement(
    model: &dyn wishpool_review::ReviewModel,
    openalex: &wishpool_review::openalex::OpenAlex,
    paper: &SearchedPaper<'_>,
    claim: &wishpool_core::ids::ClaimId,
    statement: &str,
) -> wishpool_review::ReviewResult<(mapping::Found, wishpool_review::Usage)> {
    let (mut queries, mut usage) = model
        .search_queries(paper.title, paper.abstract_text, statement)
        .await?;
    if queries.is_empty() {
        queries.push(crate::latex::search_query(statement, paper.title));
    }
    let mut candidates: Vec<wishpool_review::openalex::Work> = Vec::new();
    for query in &queries {
        for work in openalex.search(query, CANDIDATES_PER_QUERY).await? {
            if candidates.len() < CANDIDATES_PER_STATEMENT
                && !is_same_paper(&work, paper)
                && !candidates.iter().any(|c| c.id == work.id)
            {
                candidates.push(work);
            }
        }
    }
    let relations = if candidates.is_empty() {
        vec![]
    } else {
        let (relations, more) = model.relate_candidates(statement, &candidates).await?;
        usage.input += more.input;
        usage.output += more.output;
        relations
    };
    Ok((
        mapping::Found {
            claim: claim.clone(),
            queries,
            candidates,
            relations,
        },
        usage,
    ))
}

fn normalized(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether a search result is the submitted paper itself (or a companion
/// entry with its title, such as a reproducibility package).
fn is_same_paper(work: &wishpool_review::openalex::Work, paper: &SearchedPaper<'_>) -> bool {
    let title = normalized(paper.title);
    if !title.is_empty() && normalized(&work.title).starts_with(&title) {
        return true;
    }
    match (paper.doi, &work.doi) {
        (Some(paper_doi), Some(work_doi)) => {
            let paper_doi = normalise_doi(paper_doi);
            !paper_doi.is_empty()
                && paper_doi.to_lowercase() == normalise_doi(work_doi).to_lowercase()
        }
        _ => false,
    }
}

pub const MAX_ATTEMPTS: u32 = 3;

// Keep the old module paths available to the binary and its tests while the
// port itself lives with the other Layer 2 ports.
pub use wishpool_core::ports::{JobLease, LeasedJob};

/// The stored form of a job kind: `compile` or `stage:<stage>`.
pub fn kind_key(kind: JobKind) -> String {
    match kind {
        JobKind::Compile => "compile".to_owned(),
        JobKind::Referee => "referee".to_owned(),
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
mod search_tests {
    use wishpool_review::openalex::Work;

    use super::{SearchedPaper, is_same_paper};

    fn work(title: &str, doi: Option<&str>) -> Work {
        Work {
            id: "W1".into(),
            title: title.into(),
            doi: doi.map(str::to_owned),
            year: Some(2026),
            abstract_text: String::new(),
        }
    }

    #[test]
    fn excludes_the_paper_and_its_companions() {
        let paper = SearchedPaper {
            title: "A Padovan-automatic description of a nested recurrence",
            abstract_text: "",
            doi: Some("10.48550/arXiv.2609.33421"),
        };
        assert!(is_same_paper(
            &work(
                "A Padovan-Automatic Description of a Nested Recurrence",
                None
            ),
            &paper
        ));
        assert!(is_same_paper(
            &work(
                "A Padovan-automatic description of a nested recurrence: reproducibility package",
                None
            ),
            &paper
        ));
        assert!(is_same_paper(
            &work(
                "Preprint",
                Some("https://doi.org/10.48550/arXiv.2609.33421")
            ),
            &paper
        ));
        assert!(!is_same_paper(
            &work("An exploration of nested recurrences", None),
            &paper
        ));
    }

    #[test]
    fn matches_normalised_dois_exactly_ignoring_case() {
        let paper = SearchedPaper {
            title: "",
            abstract_text: "",
            doi: Some("  DOI: 10.1000/AbC  "),
        };
        for doi in [
            "10.1000/abc",
            " https://doi.org/10.1000/ABC ",
            "http://dx.doi.org/10.1000/abc",
        ] {
            assert!(is_same_paper(&work("Other title", Some(doi)), &paper));
        }
        for doi in ["10.1000/abc2", "10.9999/10.1000/abc", "10.1000/ab", ""] {
            assert!(!is_same_paper(&work("Other title", Some(doi)), &paper));
        }
        assert!(!is_same_paper(&work("Other title", None), &paper));
        let paper = SearchedPaper {
            doi: Some(" doi: "),
            ..paper
        };
        assert!(!is_same_paper(&work("Other title", Some("")), &paper));
        let paper = SearchedPaper { doi: None, ..paper };
        assert!(!is_same_paper(
            &work("Other title", Some("10.1000/abc")),
            &paper
        ));
        let paper = SearchedPaper {
            doi: Some("10.1000/Ä"),
            ..paper
        };
        assert!(is_same_paper(
            &work("Other title", Some("10.1000/ä")),
            &paper
        ));
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
