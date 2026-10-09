//! The editors' side: stage reports, escape judgements, endorsement, and
//! applying the publication threshold.

use chrono::Datelike;
use serde::{Deserialize, Serialize};

use super::App;
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, EndorsementId, RecordId, SubmissionId, new_uuid},
    judgement::{ClaimJudgement, PaperAnalysis, Standing, combine},
    model::*,
    policy::Decision,
    ports::JobKind,
};

/// The engine behind a machine report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiledBy {
    pub engine: String,
    #[serde(default)]
    pub model: Option<String>,
}

impl App {
    /// File a report on S0, S2 or S3. Editors file human reports; reviewer
    /// accounts file machine reports and name their engine. S1 belongs to
    /// the author.
    pub async fn file_report(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        stage: Stage,
        draft: ReportDraft,
        filed_by: Option<FiledBy>,
    ) -> CoreResult<Submission> {
        if stage == Stage::Claims {
            return Err(CoreError::invalid("statements are confirmed by the author"));
        }
        let reviewer = if caller.require(Role::Editor).is_ok() && filed_by.is_none() {
            ReviewerIdentity::Human {
                person: caller.person.clone(),
            }
        } else if caller.has(Role::Reviewer) {
            let filed_by = filed_by
                .ok_or_else(|| CoreError::invalid("machine reports must name their engine"))?;
            ReviewerIdentity::Machine {
                account: caller.person.clone(),
                engine: filed_by.engine,
                model: filed_by.model,
            }
        } else {
            return Err(CoreError::forbidden(
                "filing reports requires the Editor or Reviewer role",
            ));
        };
        let mut submission = self.submission(caller, id).await?;
        if !submission.is_open() {
            return Err(CoreError::conflict(format!(
                "the paper is {}",
                submission.status.name()
            )));
        }
        if submission.involves(&caller.person) {
            return Err(CoreError::forbidden(
                "authors cannot review their own paper",
            ));
        }
        if let Some(blocking) = Stage::ALL.into_iter().take_while(|s| *s < stage).find(|s| {
            !matches!(
                submission.latest_report(*s).map(|r| &r.outcome),
                Some(Outcome::Pass)
            )
        }) {
            return Err(CoreError::conflict(format!(
                "{} has not passed; {} cannot be reported yet",
                blocking.code(),
                stage.code()
            )));
        }
        draft.validate(stage, &submission.claims)?;
        submission.push_report(StageReport {
            stage,
            outcome: draft.outcome,
            summary: draft.summary,
            payload: draft.payload,
            evidence: draft.evidence,
            reviewer,
            claims_revision: submission.claims_revision,
            filed_at: self.ports.clock.now(),
        });
        let endorsements = self.ports.endorsements.for_submission(id).await?;
        submission.decision = Some(self.policy.decide(&submission, &endorsements));
        self.save(&mut submission).await?;
        if let Some(next) = self.policy.next_machine_stage(&submission) {
            self.ports.queue.enqueue(id, JobKind::Stage(next)).await?;
        }
        Ok(submission)
    }

    /// The analysis of a paper, for its authors and staff.
    pub async fn analysis(&self, caller: &Caller, id: &SubmissionId) -> CoreResult<PaperAnalysis> {
        let submission = self.submission(caller, id).await?;
        let judgements = self.ports.judgements.for_submission(id).await?;
        Ok(PaperAnalysis::compute(&submission, &judgements))
    }

    /// The judgements of the current statements, for authors and staff.
    pub async fn judgements(
        &self,
        caller: &Caller,
        id: &SubmissionId,
    ) -> CoreResult<Vec<ClaimJudgement>> {
        let submission = self.submission(caller, id).await?;
        let mut judgements = self.ports.judgements.for_submission(id).await?;
        judgements.retain(|j| j.claims_revision == submission.claims_revision);
        Ok(judgements)
    }

    /// An editor's own escape judgement of a statement. It confirms or
    /// overrides machine judgements and settles open judgement tasks.
    pub async fn judge_claim(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        claim: &ClaimId,
        shape: ProofShape,
        witnesses: Vec<String>,
        rationale: String,
    ) -> CoreResult<ClaimJudgement> {
        caller.require(Role::Editor)?;
        ContributionOutput::Judgement {
            shape,
            witnesses: witnesses.clone(),
            rationale: rationale.clone(),
        }
        .validate(TaskKind::JudgeEscape)?;
        let submission = self.submission(caller, id).await?;
        if submission.involves(&caller.person) {
            return Err(CoreError::forbidden("authors cannot judge their own paper"));
        }
        if submission.claim(claim).is_none() {
            return Err(CoreError::not_found("statement", claim.as_str()));
        }
        let judgement = ClaimJudgement {
            id: new_uuid(),
            submission: id.clone(),
            claim: claim.clone(),
            claims_revision: submission.claims_revision,
            shape,
            witnesses,
            rationale,
            reviewer: ReviewerIdentity::Human {
                person: caller.person.clone(),
            },
            task: None,
            filed_at: self.ports.clock.now(),
        };
        self.ports.judgements.insert(&judgement).await?;
        self.settle_judge_tasks(id, claim).await?;
        Ok(judgement)
    }

    /// A reviewer account's machine judgement of a statement: a proposal
    /// that counts towards a quorum and that an editor confirms or overrides.
    pub async fn file_machine_judgement(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        claim: &ClaimId,
        output: ContributionOutput,
        filed_by: FiledBy,
    ) -> CoreResult<ClaimJudgement> {
        caller.require(Role::Reviewer)?;
        output.validate(TaskKind::JudgeEscape)?;
        let ContributionOutput::Judgement {
            shape,
            witnesses,
            rationale,
        } = output
        else {
            return Err(CoreError::invalid("expected a judgement"));
        };
        let submission = self.submission(caller, id).await?;
        if !submission.is_open() {
            return Err(CoreError::conflict(format!(
                "the paper is {}",
                submission.status.name()
            )));
        }
        if submission.claim(claim).is_none_or(|c| c.kind.is_open()) {
            return Err(CoreError::not_found("proved statement", claim.as_str()));
        }
        let judgement = ClaimJudgement {
            id: new_uuid(),
            submission: id.clone(),
            claim: claim.clone(),
            claims_revision: submission.claims_revision,
            shape,
            witnesses,
            rationale,
            reviewer: ReviewerIdentity::Machine {
                account: caller.person.clone(),
                engine: filed_by.engine,
                model: filed_by.model,
            },
            task: None,
            filed_at: self.ports.clock.now(),
        };
        self.ports.judgements.insert(&judgement).await?;
        self.settle_judge_tasks(id, claim).await?;
        Ok(judgement)
    }

    /// File the escape report (S3) from the judgements gathered: every
    /// statement with a confirmed or corroborated judgement is included; every
    /// main result must have one. The editor's report makes them binding.
    pub async fn adopt_judgements(
        &self,
        caller: &Caller,
        id: &SubmissionId,
    ) -> CoreResult<Submission> {
        caller.require(Role::Editor)?;
        let submission = self.submission(caller, id).await?;
        let judgements = self.ports.judgements.for_submission(id).await?;
        let mut assessments = Vec::new();
        let mut missing = Vec::new();
        for claim in submission.claims.iter().filter(|c| !c.kind.is_open()) {
            let mine: Vec<&ClaimJudgement> = judgements
                .iter()
                .filter(|j| j.claim == claim.id && j.claims_revision == submission.claims_revision)
                .collect();
            match combine(&mine) {
                Some(c) if matches!(c.standing, Standing::Confirmed | Standing::Corroborated) => {
                    let shape = c.shape.unwrap_or(ProofShape::BindOnly);
                    let source = mine.iter().rev().find(|j| j.shape == shape);
                    assessments.push(EscapeAssessment {
                        claim: claim.id.clone(),
                        shape,
                        witnesses: if shape == ProofShape::Content {
                            c.witnesses.clone()
                        } else {
                            vec![]
                        },
                        rationale: source.map(|j| j.rationale.clone()).unwrap_or_default(),
                        escape_rate: None,
                    });
                }
                _ if claim.is_main_result() => missing.push(claim.id.to_string()),
                _ => {}
            }
        }
        if !missing.is_empty() {
            return Err(CoreError::conflict(format!(
                "main results without a settled judgement: {}",
                missing.join(", ")
            )));
        }
        let draft = ReportDraft {
            outcome: Outcome::Pass,
            summary: format!(
                "{} statements judged; adopted by the editor.",
                assessments.len()
            ),
            payload: StagePayload::Escape { assessments },
            evidence: vec![],
        };
        self.file_report(caller, id, Stage::Escape, draft, None)
            .await
    }

    pub async fn endorse(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        new: NewEndorsement,
    ) -> CoreResult<Endorsement> {
        if !caller.has(Role::Endorser) {
            return Err(CoreError::forbidden("endorsing requires the Endorser role"));
        }
        new.validate()?;
        let submission = self.load(id).await?;
        if !submission.is_open() {
            return Err(CoreError::conflict(format!(
                "the paper is {}",
                submission.status.name()
            )));
        }
        let endorsement = Endorsement {
            id: EndorsementId(new_uuid()),
            submission: submission.id.clone(),
            endorser: caller.person.clone(),
            significance: new.significance,
            conflicts: new.conflicts,
            is_author: submission.involves(&caller.person),
            filed_at: self.ports.clock.now(),
        };
        self.ports.endorsements.insert(&endorsement).await?;
        Ok(endorsement)
    }

    /// Endorsements of a paper, for its authors and staff.
    pub async fn endorsements(
        &self,
        caller: &Caller,
        id: &SubmissionId,
    ) -> CoreResult<Vec<Endorsement>> {
        self.submission(caller, id).await?;
        self.ports.endorsements.for_submission(id).await
    }

    pub async fn preview_decision(
        &self,
        caller: &Caller,
        id: &SubmissionId,
    ) -> CoreResult<Decision> {
        let submission = self.submission(caller, id).await?;
        let endorsements = self.ports.endorsements.for_submission(id).await?;
        Ok(self.policy.decide(&submission, &endorsements))
    }

    /// Apply the threshold. Accepted papers get a record and a public page;
    /// the others keep a private report and may be revised.
    pub async fn decide(&self, caller: &Caller, id: &SubmissionId) -> CoreResult<Submission> {
        caller.require(Role::Editor)?;
        let mut submission = self.submission(caller, id).await?;
        if !submission.is_open() {
            return Err(CoreError::conflict(format!(
                "the paper is {}",
                submission.status.name()
            )));
        }
        if submission.involves(&caller.person) {
            return Err(CoreError::forbidden(
                "authors cannot decide their own paper",
            ));
        }
        let endorsements = self.ports.endorsements.for_submission(id).await?;
        let decision = self.policy.decide(&submission, &endorsements);
        let now = self.ports.clock.now();
        match &decision {
            Decision::Pending { awaiting, detail } => {
                return Err(CoreError::conflict(format!(
                    "awaiting {}: {detail}",
                    awaiting.code()
                )));
            }
            Decision::NotAccepted { .. } => submission.status = SubmissionStatus::NotAccepted,
            Decision::Accept { basis } => {
                let year = now.year();
                let record = Record {
                    id: RecordId::format(year, self.ports.records.next_sequence(year).await?),
                    submission: submission.id.clone(),
                    title: submission.title.clone(),
                    authors: submission.authors.clone(),
                    basis: *basis,
                    accepted_by: caller.person.clone(),
                    accepted_at: now,
                };
                self.ports.records.insert(&record).await?;
                submission.status = SubmissionStatus::Accepted { record: record.id };
                submission.conjectures = submission
                    .claims
                    .iter()
                    .filter(|c| c.kind.is_open())
                    .map(|c| ConjectureFollowUp {
                        claim: c.id.clone(),
                        state: ConjectureState::Screening,
                        updated_at: now,
                    })
                    .collect();
            }
        }
        submission.decision = Some(decision);
        self.save(&mut submission).await?;
        Ok(submission)
    }
}
