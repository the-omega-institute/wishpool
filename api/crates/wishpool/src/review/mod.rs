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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeasedJob {
    pub submission: SubmissionId,
    pub kind: JobKind,
    pub attempts: u32,
    /// Fences completion against a lease that has since been re-issued.
    pub lease: String,
}

#[async_trait]
pub trait JobLease: Send + Sync {
    /// Lease the oldest available job.
    async fn claim(&self) -> CoreResult<Option<LeasedJob>>;
    async fn complete(&self, job: &LeasedJob) -> CoreResult<()>;
    /// Release a waiting job without recording a failure or consuming an attempt.
    async fn defer(&self, job: &LeasedJob, delay: std::time::Duration) -> CoreResult<()>;
    /// Return the job for a later retry, or park it as failed after
    /// [`MAX_ATTEMPTS`].
    async fn retry(&self, job: &LeasedJob, error: &str) -> CoreResult<()>;
}

/// The stored form of a job kind: `compile` or `stage:<stage>`.
pub fn kind_key(kind: JobKind) -> String {
    match kind {
        JobKind::Compile => "compile".to_owned(),
        JobKind::Referee => "referee".to_owned(),
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

/// Memory-mode job source over the core memory stores.
pub struct MemoryJobs {
    pub stores: std::sync::Arc<wishpool_core::memory::MemoryStores>,
    deferred: tokio::sync::Mutex<Vec<(tokio::time::Instant, LeasedJob)>>,
}

impl MemoryJobs {
    pub fn new(stores: std::sync::Arc<wishpool_core::memory::MemoryStores>) -> Self {
        Self {
            stores,
            deferred: Default::default(),
        }
    }
}

#[async_trait]
impl JobLease for MemoryJobs {
    async fn claim(&self) -> CoreResult<Option<LeasedJob>> {
        let mut deferred = self.deferred.lock().await;
        if let Some(index) = deferred
            .iter()
            .position(|(at, _)| *at <= tokio::time::Instant::now())
        {
            return Ok(Some(deferred.remove(index).1));
        }
        Ok(self.stores.take_job().await.map(|job| LeasedJob {
            submission: job.submission,
            kind: job.kind,
            attempts: 1,
            lease: String::new(),
        }))
    }

    async fn complete(&self, _job: &LeasedJob) -> CoreResult<()> {
        Ok(())
    }

    async fn defer(&self, job: &LeasedJob, delay: std::time::Duration) -> CoreResult<()> {
        self.deferred
            .lock()
            .await
            .push((tokio::time::Instant::now() + delay, job.clone()));
        Ok(())
    }

    async fn retry(&self, job: &LeasedJob, error: &str) -> CoreResult<()> {
        tracing::warn!(submission = %job.submission, kind = kind_key(job.kind), error, "memory-mode job dropped after failure");
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

    #[tokio::test]
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
        jobs.defer(&job, Duration::ZERO).await.unwrap();
        assert_eq!(jobs.claim().await.unwrap().unwrap().attempts, job.attempts);
    }
}
