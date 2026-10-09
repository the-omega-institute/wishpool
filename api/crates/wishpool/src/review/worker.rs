use std::{sync::Arc, time::Duration};

use tokio::sync::watch;
use tracing::Instrument;
use wishpool_core::{
    CoreError,
    app::{App, FiledBy},
    model::{Caller, ReviewerIdentity, Stage, Submission, SubmissionStatus},
    ports::JobKind,
};
use wishpool_review::{Document, ReviewModel, openalex::OpenAlex};

use super::{JobLease, LeasedJob, SearchedPaper, kind_key, mapping, search_statement};
use crate::latex::{self, Compile};

const IDLE_POLL: Duration = Duration::from_secs(3);
const RECONCILE_INTERVAL: Duration = Duration::from_secs(60);
/// Main results searched per paper.
const MAX_SEARCHED: usize = 6;

pub struct Worker {
    pub app: Arc<App>,
    pub jobs: Arc<dyn JobLease>,
    pub compile: Compile,
    pub model: Option<Arc<dyn ReviewModel>>,
    pub openalex: Arc<OpenAlex>,
    pub reviewer: Caller,
    pub referee_account: Caller,
    pub referee_model: String,
    pub oracle: Option<Arc<dyn wishpool_review::oracle::Oracle>>,
    pub advisor: Option<Arc<dyn wishpool_review::advisor::Advisor>>,
    pub formalizer: Option<Arc<dyn wishpool_review::lean::Formalizer>>,
    pub oracle_poll: Duration,
    pub advisor_work_dir: std::path::PathBuf,
}

/// Whether a failure is worth retrying.
#[derive(Debug)]
pub(super) enum Failure {
    Transient(String),
    Permanent(String),
}

impl From<CoreError> for Failure {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::Unavailable(_) | CoreError::StaleRevision { .. } => {
                Self::Transient(error.to_string())
            }
            other => Self::Permanent(other.to_string()),
        }
    }
}

fn model_failure(error: wishpool_review::ReviewError) -> Failure {
    match error {
        wishpool_review::ReviewError::Transport(e) => Failure::Transient(e),
        wishpool_review::ReviewError::Provider { status, body }
            if status == 429 || status >= 500 =>
        {
            Failure::Transient(format!("{status}: {body}"))
        }
        other => Failure::Permanent(other.to_string()),
    }
}

impl Worker {
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tracing::info!(
            model = self.model.as_ref().map(|m| m.model()),
            "paper worker started"
        );
        loop {
            if *shutdown.borrow() {
                break;
            }
            match self.jobs.claim().await {
                Ok(Some(job)) => self.handle(job).await,
                Ok(None) => {
                    tokio::select! {
                        _ = tokio::time::sleep(IDLE_POLL) => {}
                        _ = shutdown.changed() => {}
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "could not claim a job");
                    tokio::time::sleep(IDLE_POLL).await;
                }
            }
        }
        tracing::info!("paper worker stopped");
    }

    async fn handle(&self, job: LeasedJob) {
        let span = tracing::info_span!("paper_job", submission = %job.submission, kind = kind_key(job.kind), attempt = job.attempts);
        let result = self.process(&job).instrument(span).await;
        let settled = match result {
            Ok(true) => Ok(()),
            Ok(false) => self.jobs.complete(&job).await,
            Err(Failure::Permanent(reason)) => {
                tracing::warn!(reason, "job abandoned");
                self.jobs.complete(&job).await
            }
            Err(Failure::Transient(reason)) => {
                tracing::warn!(reason, "job will be retried");
                self.jobs.retry(&job, &reason).await
            }
        };
        if let Err(error) = settled {
            tracing::error!(%error, "could not settle job; its lease will expire and it will be retried");
        }
    }

    async fn process(&self, job: &LeasedJob) -> Result<bool, Failure> {
        if job.kind == JobKind::Referee {
            return self.referee_job(job).await;
        }
        let submission = match self.app.submission(&self.reviewer, &job.submission).await {
            Ok(submission) => submission,
            Err(CoreError::NotFound { .. }) => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        match job.kind {
            JobKind::Referee => unreachable!("referee handled above"),
            JobKind::Compile => self.compile(&submission).await,
            JobKind::Stage(Stage::Literature) => self.literature(&submission).await,
            JobKind::Stage(Stage::Escape) => self.escape(&submission).await,
            // The compile job files S0; the author confirms S1.
            JobKind::Stage(Stage::Hygiene | Stage::Claims) => Ok(()),
        }?;
        Ok(false)
    }

    async fn source(&self, submission: &Submission) -> Result<(Vec<u8>, String), Failure> {
        let version = submission
            .current_version()
            .ok_or_else(|| Failure::Permanent("the paper has no version".into()))?;
        let file = self
            .app
            .paper_file(
                Some(&self.reviewer),
                &submission.id,
                Some(version.number),
                false,
            )
            .await?;
        Ok((file.bytes, version.filename.clone()))
    }

    async fn compile(&self, submission: &Submission) -> Result<(), Failure> {
        if submission.status == SubmissionStatus::Withdrawn {
            return Ok(());
        }
        let Some(version) = submission.current_version() else {
            return Ok(());
        };
        if version.pdf.is_some() || version.compile_error.is_some() {
            return Ok(());
        }
        let number = version.number;
        let (bytes, filename) = self.source(submission).await?;
        let outcome = self.compile.pdf(bytes, filename).await;
        if let Err(message) = &outcome {
            tracing::info!(version = number, message = %message.lines().last().unwrap_or_default(), "source did not compile");
        }
        self.app
            .record_compilation(&self.reviewer, &submission.id, number, outcome)
            .await?;
        Ok(())
    }

    async fn literature(&self, submission: &Submission) -> Result<(), Failure> {
        let Some(model) = &self.model else {
            return Ok(());
        };
        if !submission.is_open() || submission.latest_report(Stage::Literature).is_some() {
            return Ok(());
        }
        let paper = SearchedPaper {
            title: &submission.title,
            abstract_text: &submission.abstract_text,
            doi: submission.doi.as_deref(),
        };
        let mut found = Vec::new();
        for claim in submission
            .claims
            .iter()
            .filter(|c| c.is_main_result())
            .take(MAX_SEARCHED)
        {
            let (leads, _) = search_statement(
                model.as_ref(),
                &self.openalex,
                &paper,
                &claim.id,
                &claim.statement,
            )
            .await
            .map_err(model_failure)?;
            found.push(leads);
        }
        let draft = mapping::literature(&found, model.model());
        self.app
            .file_report(
                &self.reviewer,
                &submission.id,
                Stage::Literature,
                draft,
                Some(self.filed_by(model.as_ref())),
            )
            .await?;
        Ok(())
    }

    fn filed_by(&self, model: &dyn ReviewModel) -> FiledBy {
        FiledBy {
            engine: model.engine().to_owned(),
            model: Some(model.model().to_owned()),
        }
    }

    async fn escape(&self, submission: &Submission) -> Result<(), Failure> {
        let Some(model) = &self.model else {
            return Ok(());
        };
        if !submission.is_open() {
            return Ok(());
        }
        // Once per claim set: skip when this account already judged it.
        let judged = self
            .app
            .judgements(&self.reviewer, &submission.id)
            .await?
            .iter()
            .any(|j| matches!(&j.reviewer, ReviewerIdentity::Machine { account, .. } if account == &self.reviewer.person));
        if judged {
            return Ok(());
        }
        let (bytes, filename) = self.source(submission).await?;
        let document = Document {
            title: submission.title.clone(),
            abstract_text: submission.abstract_text.clone(),
            text: latex::source_text(&bytes, &filename).ok(),
        };
        let (proposals, _usage) = model
            .assess_escape(&document, &mapping::statements(&submission.claims))
            .await
            .map_err(model_failure)?;
        let (kept, skipped) = mapping::proposals(&proposals, &submission.claims);
        if !skipped.is_empty() {
            tracing::info!(skipped = skipped.join(", "), "escape proposals dropped");
        }
        for (claim, output) in kept {
            self.app
                .file_machine_judgement(
                    &self.reviewer,
                    &submission.id,
                    &claim,
                    output,
                    self.filed_by(model.as_ref()),
                )
                .await?;
        }
        Ok(())
    }
}

/// Return expired task leases to the pool.
pub async fn run_reconciler(app: Arc<App>, mut shutdown: watch::Receiver<bool>) {
    loop {
        match app.reconcile().await {
            Ok(outcome) if outcome.leases_expired > 0 => {
                tracing::info!(expired = outcome.leases_expired, "task leases expired");
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "reconcile failed"),
        }
        tokio::select! {
            _ = tokio::time::sleep(RECONCILE_INTERVAL) => {}
            _ = shutdown.changed() => break,
        }
    }
}
