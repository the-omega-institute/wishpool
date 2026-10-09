//! The author's side: upload a LaTeX source, confirm the statements read
//! from it, revise, withdraw, and decide who sees the analysis.

use serde::{Deserialize, Serialize};

use super::{App, clamp};
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, SubmissionId, new_uuid},
    model::*,
    ports::{JobKind, Listing, ReadPaper, SubmissionFilter},
};

/// Largest source upload accepted.
pub const MAX_UPLOAD_BYTES: usize = 30 * 1024 * 1024;

pub struct Upload {
    pub filename: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionScope {
    /// Papers the caller submitted or is a linked author of.
    Mine,
    /// Papers in review; editors and reviewers only.
    Queue,
}

/// A file to hand back: content type and bytes.
pub struct PaperFile {
    pub content_type: &'static str,
    pub filename: String,
    pub bytes: Vec<u8>,
}

/// Statements read from the source, before the author confirms them.
pub(crate) fn claims_from(read: &ReadPaper) -> Vec<Claim> {
    let any_theorem = read.statements.iter().any(|s| s.kind == ClaimKind::Theorem);
    let mut ordinals: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    read.statements
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let n = ordinals.entry(s.display_name.as_str()).or_insert(0);
            *n += 1;
            let label = match (&s.title, &s.latex_label) {
                (Some(title), _) => format!("{} ({title})", s.display_name),
                (None, Some(label)) => format!("{} [{label}]", s.display_name),
                (None, None) => format!("{} {}", s.display_name, n),
            };
            // Theorems are main results by default; without any theorem,
            // propositions are. The author confirms or changes this.
            let main = match s.kind {
                ClaimKind::Theorem => true,
                ClaimKind::Proposition => !any_theorem,
                _ => false,
            };
            Claim {
                id: ClaimId(format!("C{}", i + 1)),
                kind: s.kind,
                label,
                latex_label: s.latex_label.clone(),
                statement: s.body.chars().take(40_000).collect(),
                role: if main {
                    ClaimRole::Main
                } else {
                    ClaimRole::Supporting
                },
                has_proof: s.has_proof,
                section: s.section.clone(),
                depends_on: vec![],
                settles: None,
            }
        })
        .collect()
}

impl App {
    fn read(&self, upload: &Upload) -> CoreResult<ReadPaper> {
        if upload.bytes.is_empty() {
            return Err(CoreError::invalid("the upload is empty"));
        }
        if upload.bytes.len() > MAX_UPLOAD_BYTES {
            return Err(CoreError::invalid(format!(
                "the upload exceeds {} MB",
                MAX_UPLOAD_BYTES / (1024 * 1024)
            )));
        }
        let read = self
            .ports
            .reader
            .read(&upload.bytes, &upload.filename)
            .map_err(CoreError::Invalid)?;
        if read.statements.is_empty() {
            return Err(CoreError::invalid(
                "no theorem, lemma, proposition, corollary or conjecture environments were found; declare them with \\newtheorem",
            ));
        }
        Ok(read)
    }

    async fn new_version(
        &self,
        number: u32,
        upload: &Upload,
        read: &ReadPaper,
        note: String,
    ) -> CoreResult<PaperVersion> {
        let archive = self
            .ports
            .blobs
            .put(&upload.bytes, "application/octet-stream")
            .await?;
        Ok(PaperVersion {
            number,
            archive,
            filename: upload.filename.chars().take(200).collect(),
            main_file: read.main_file.clone(),
            pdf: None,
            compile_error: None,
            parse_warnings: read.warnings.clone(),
            macros: read.macros.clone(),
            note,
            uploaded_at: self.ports.clock.now(),
        })
    }

    /// Upload a paper. The statements read from the source await the
    /// author's confirmation; the PDF is compiled in the background.
    pub async fn submit_paper(
        &self,
        caller: &Caller,
        mut new: NewPaper,
        upload: Upload,
    ) -> CoreResult<Submission> {
        new.normalise_doi();
        new.validate()?;
        let active = self.ports.submissions.active_count(&caller.person).await?;
        if active as usize >= self.policy.max_active_per_author {
            return Err(CoreError::conflict(format!(
                "you have {active} papers in draft or review; at most {} at once",
                self.policy.max_active_per_author
            )));
        }
        let read = self.read(&upload)?;
        let title = read
            .title
            .clone()
            .ok_or_else(|| CoreError::invalid("the source has no \\title"))?;
        let authors = if new.authors.is_empty() {
            read.authors
                .iter()
                .map(|name| Author {
                    name: name.clone(),
                    person: None,
                    orcid: None,
                    affiliation: None,
                })
                .collect()
        } else {
            new.authors.clone()
        };
        if authors.is_empty() {
            return Err(CoreError::invalid(
                "the source has no \\author; list the authors",
            ));
        }
        let now = self.ports.clock.now();
        let version = self.new_version(1, &upload, &read, String::new()).await?;
        let submission = Submission {
            id: SubmissionId(new_uuid()),
            submitter: caller.person.clone(),
            title,
            abstract_text: read.abstract_text.clone().unwrap_or_default(),
            authors,
            ai_disclosure: new.ai_disclosure,
            msc: new.msc,
            doi: new.doi,
            versions: vec![version],
            extracted: claims_from(&read),
            claims: vec![],
            claims_revision: 0,
            reports: vec![],
            status: SubmissionStatus::Draft,
            decision: None,
            open_to_contributors: new.open_to_contributors,
            analysis_visibility: Visibility::Undecided,
            formalization: FormalizationPlan::default(),
            conjectures: vec![],
            created_at: now,
            updated_at: now,
            revision: 0,
        };
        self.ports.submissions.insert(&submission).await?;
        self.ports
            .queue
            .enqueue(&submission.id, JobKind::Compile)
            .await?;
        Ok(submission)
    }

    /// The submitting author passes; co-authors and staff are refused;
    /// anyone else is told the paper does not exist.
    pub(crate) fn require_submitter(caller: &Caller, submission: &Submission) -> CoreResult<()> {
        if submission.submitter == caller.person {
            Ok(())
        } else if Self::can_view(caller, submission) {
            Err(CoreError::forbidden(
                "only the submitting author can do this",
            ))
        } else {
            Err(CoreError::not_found("paper", submission.id.as_str()))
        }
    }

    /// The author confirms which statements to analyse and which are main
    /// results. Analysis starts after this.
    pub async fn confirm_claims(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        confirmations: Vec<ClaimConfirmation>,
    ) -> CoreResult<Submission> {
        let mut submission = self.load(id).await?;
        Self::require_submitter(caller, &submission)?;
        if !submission.status.is_active() {
            return Err(CoreError::conflict(format!(
                "the paper is {}",
                submission.status.name()
            )));
        }
        let mut claims = Vec::new();
        for claim in &submission.extracted {
            let Some(c) = confirmations.iter().find(|c| c.id == claim.id) else {
                return Err(CoreError::invalid(format!(
                    "statement {} is not confirmed or excluded",
                    claim.id
                )));
            };
            if c.excluded {
                continue;
            }
            let mut claim = claim.clone();
            claim.kind = c.kind;
            claim.role = c.role;
            claim.depends_on = c.depends_on.clone();
            claim.settles = c.settles.clone();
            claims.push(claim);
        }
        if let Some(unknown) = confirmations
            .iter()
            .find(|c| !submission.extracted.iter().any(|e| e.id == c.id))
        {
            return Err(CoreError::invalid(format!(
                "unknown statement {}",
                unknown.id
            )));
        }
        // Dependencies may only point at statements that remain.
        for claim in &mut claims.clone() {
            if let Some(missing) = claim
                .depends_on
                .iter()
                .find(|d| !claims.iter().any(|c| &c.id == *d))
            {
                return Err(CoreError::invalid(format!(
                    "{} depends on excluded statement {missing}",
                    claim.id
                )));
            }
        }
        validate_claims(&claims)?;
        if !claims.iter().any(|c| c.is_main_result()) {
            return Err(CoreError::invalid(
                "mark at least one proved statement as a main result",
            ));
        }
        let now = self.ports.clock.now();
        let before = submission.claims_revision;
        submission.push_report(StageReport {
            stage: Stage::Claims,
            outcome: Outcome::Pass,
            summary: format!("{} statements confirmed by the author.", claims.len()),
            payload: StagePayload::Claims { claims },
            evidence: vec![],
            reviewer: ReviewerIdentity::Human {
                person: caller.person.clone(),
            },
            claims_revision: submission.claims_revision,
            filed_at: now,
        });
        submission.status = SubmissionStatus::InReview;
        let endorsements = self.ports.endorsements.for_submission(id).await?;
        submission.decision = Some(self.policy.decide(&submission, &endorsements));
        self.save(&mut submission).await?;
        if let Some(stage) = self.policy.next_machine_stage(&submission) {
            self.ports.queue.enqueue(id, JobKind::Stage(stage)).await?;
        }
        self.ports.queue.enqueue(id, JobKind::Referee).await?;
        if submission.claims_revision != before {
            self.close_tasks(id, "the statements changed").await?;
        }
        self.cut_review_tasks(&caller.person, &submission).await?;
        Ok(submission)
    }

    /// Upload a revised source. Statements are read again and await the
    /// author's confirmation; reports on unchanged statements stay valid.
    pub async fn upload_version(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        upload: Upload,
        note: String,
    ) -> CoreResult<Submission> {
        let mut submission = self.load(id).await?;
        Self::require_submitter(caller, &submission)?;
        if !matches!(
            submission.status,
            SubmissionStatus::Draft | SubmissionStatus::InReview | SubmissionStatus::NotAccepted
        ) {
            return Err(CoreError::conflict(format!(
                "the paper is {}",
                submission.status.name()
            )));
        }
        if submission.status == SubmissionStatus::NotAccepted {
            let active = self.ports.submissions.active_count(&caller.person).await?;
            if active as usize >= self.policy.max_active_per_author {
                return Err(CoreError::conflict(
                    "too many papers in draft or review to resubmit now",
                ));
            }
        }
        let read = self.read(&upload)?;
        let number = submission.versions.len() as u32 + 1;
        let version = self
            .new_version(number, &upload, &read, note.chars().take(2_000).collect())
            .await?;
        submission.versions.push(version);
        submission.extracted = claims_from(&read);
        if let Some(title) = read.title.clone() {
            submission.title = title;
        }
        if let Some(abstract_text) = read.abstract_text.clone() {
            submission.abstract_text = abstract_text;
        }
        submission.status = SubmissionStatus::Draft;
        submission.decision = None;
        self.save(&mut submission).await?;
        self.ports.queue.enqueue(id, JobKind::Compile).await?;
        Ok(submission)
    }

    pub async fn withdraw(&self, caller: &Caller, id: &SubmissionId) -> CoreResult<Submission> {
        let mut submission = self.load(id).await?;
        Self::require_submitter(caller, &submission)?;
        if !submission.status.is_active() {
            return Err(CoreError::conflict(format!(
                "the paper is {}",
                submission.status.name()
            )));
        }
        submission.status = SubmissionStatus::Withdrawn;
        self.save(&mut submission).await?;
        self.close_tasks(id, "the author withdrew the paper")
            .await?;
        Ok(submission)
    }

    /// After acceptance the author decides whether readers see the analysis.
    pub async fn set_analysis_visibility(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        visibility: Visibility,
    ) -> CoreResult<Submission> {
        let mut submission = self.load(id).await?;
        Self::require_submitter(caller, &submission)?;
        if !matches!(submission.status, SubmissionStatus::Accepted { .. }) {
            return Err(CoreError::conflict(
                "the analysis can be published once the paper is accepted",
            ));
        }
        if visibility == Visibility::Undecided {
            return Err(CoreError::invalid("choose public or private"));
        }
        submission.analysis_visibility = visibility;
        self.save(&mut submission).await?;
        Ok(submission)
    }

    /// Whether volunteer contributors may read the statements to help.
    pub async fn set_open_to_contributors(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        open: bool,
    ) -> CoreResult<Submission> {
        let mut submission = self.load(id).await?;
        Self::require_submitter(caller, &submission)?;
        submission.open_to_contributors = open;
        self.save(&mut submission).await?;
        if open {
            self.cut_review_tasks(&caller.person, &submission).await?;
        } else {
            self.close_tasks(id, "the author closed the paper to contributors")
                .await?;
        }
        Ok(submission)
    }

    pub(crate) fn can_view(caller: &Caller, submission: &Submission) -> bool {
        submission.involves(&caller.person) || Self::is_staff(caller)
    }

    pub async fn submission(&self, caller: &Caller, id: &SubmissionId) -> CoreResult<Submission> {
        let submission = self.load(id).await?;
        if Self::can_view(caller, &submission) {
            Ok(submission)
        } else {
            Err(CoreError::not_found("paper", id.as_str()))
        }
    }

    pub async fn list_submissions(
        &self,
        caller: &Caller,
        scope: SubmissionScope,
        limit: Option<u32>,
        before: Option<String>,
    ) -> CoreResult<Listing<Submission>> {
        let filter = match scope {
            SubmissionScope::Mine => SubmissionFilter {
                involving: Some(caller.person.clone()),
                status: None,
            },
            SubmissionScope::Queue => {
                if !Self::is_staff(caller) {
                    return Err(CoreError::forbidden(
                        "the review queue is for editors and reviewers",
                    ));
                }
                SubmissionFilter {
                    involving: None,
                    status: Some("in_review".into()),
                }
            }
        };
        self.ports
            .submissions
            .list(&filter, clamp(limit), before)
            .await
    }

    /// Composition-only: store the result of compiling a version (the PDF,
    /// or the compiler's message) and file the hygiene report for the
    /// current version.
    pub async fn record_compilation(
        &self,
        reviewer: &Caller,
        id: &SubmissionId,
        version: u32,
        outcome: Result<Vec<u8>, String>,
    ) -> CoreResult<Submission> {
        reviewer.require(Role::Reviewer)?;
        let (pdf, error) = match outcome {
            Ok(bytes) => (
                Some(self.ports.blobs.put(&bytes, "application/pdf").await?),
                None,
            ),
            Err(error) => (None, Some(error)),
        };
        let mut submission = self.load(id).await?;
        let Some(entry) = submission.versions.iter_mut().find(|v| v.number == version) else {
            return Err(CoreError::not_found("version", version.to_string()));
        };
        entry.pdf = pdf.clone();
        entry.compile_error = error.clone().map(|e| e.chars().take(8_000).collect());
        let current = submission.versions.last().map(|v| v.number) == Some(version);
        if current {
            let checks = vec![
                HygieneCheck {
                    name: "source read".into(),
                    passed: true,
                    detail: submission
                        .versions
                        .last()
                        .map(|v| v.main_file.clone())
                        .unwrap_or_default(),
                },
                HygieneCheck {
                    name: "PDF compiled".into(),
                    passed: pdf.is_some(),
                    detail: error
                        .clone()
                        .unwrap_or_else(|| "TeX Live".into())
                        .chars()
                        .take(2_000)
                        .collect(),
                },
                HygieneCheck {
                    name: "AI use disclosed".into(),
                    passed: !submission.ai_disclosure.statement.trim().is_empty(),
                    detail: format!("{:?}", submission.ai_disclosure.level).to_lowercase(),
                },
            ];
            let failed: Vec<String> = checks
                .iter()
                .filter(|c| !c.passed)
                .map(|c| c.name.clone())
                .collect();
            let (outcome, summary) = if failed.is_empty() {
                (Outcome::Pass, format!("Version {version} compiles."))
            } else {
                let detail = format!("{}: upload a version that compiles", failed.join(", "));
                (
                    Outcome::Fail {
                        reason: RejectReason::Hygiene {
                            detail: detail.clone(),
                        },
                    },
                    detail,
                )
            };
            submission.push_report(StageReport {
                stage: Stage::Hygiene,
                outcome,
                summary,
                payload: StagePayload::Hygiene { checks },
                evidence: vec![],
                reviewer: ReviewerIdentity::Machine {
                    account: reviewer.person.clone(),
                    engine: "texlive".into(),
                    model: None,
                },
                claims_revision: submission.claims_revision,
                filed_at: self.ports.clock.now(),
            });
            let endorsements = self.ports.endorsements.for_submission(id).await?;
            if submission.status != SubmissionStatus::Draft {
                submission.decision = Some(self.policy.decide(&submission, &endorsements));
            }
        }
        self.save(&mut submission).await?;
        if current && let Some(stage) = self.policy.next_machine_stage(&submission) {
            self.ports.queue.enqueue(id, JobKind::Stage(stage)).await?;
        }
        Ok(submission)
    }

    /// The source archive (authors and staff) or a compiled PDF (also
    /// readers, once the paper is accepted).
    pub async fn paper_file(
        &self,
        caller: Option<&Caller>,
        id: &SubmissionId,
        version: Option<u32>,
        pdf: bool,
    ) -> CoreResult<PaperFile> {
        let submission = self.load(id).await?;
        let viewer = caller.is_some_and(|c| Self::can_view(c, &submission));
        let public = matches!(submission.status, SubmissionStatus::Accepted { .. });
        if !(viewer || (pdf && public)) {
            return Err(CoreError::not_found("paper", id.as_str()));
        }
        let entry = match version {
            Some(n) => submission.versions.iter().find(|v| v.number == n),
            None => submission.versions.last(),
        }
        .ok_or_else(|| {
            CoreError::not_found(
                "version",
                version.map(|n| n.to_string()).unwrap_or_default(),
            )
        })?;
        let (blob, content_type, filename) = if pdf {
            let blob = entry
                .pdf
                .as_ref()
                .ok_or_else(|| CoreError::not_found("pdf", entry.number.to_string()))?;
            (
                blob,
                "application/pdf",
                format!("{}-v{}.pdf", submission.id, entry.number),
            )
        } else {
            (
                &entry.archive,
                "application/octet-stream",
                entry.filename.clone(),
            )
        };
        let bytes = self
            .ports
            .blobs
            .get(&blob.id)
            .await?
            .ok_or_else(|| CoreError::Unavailable("stored file is missing".into()))?;
        Ok(PaperFile {
            content_type,
            filename,
            bytes,
        })
    }
}
