//! What readers see: accepted papers, each statement with its badges, and
//! the analysis when the author made it public.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{App, clamp};
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, RecordId, SubmissionId},
    judgement::PaperAnalysis,
    model::*,
    policy::AdmissionBasis,
    ports::Listing,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperSummary {
    pub record: RecordId,
    pub submission: SubmissionId,
    pub title: String,
    pub authors: Vec<Author>,
    pub abstract_text: String,
    pub msc: Vec<String>,
    pub doi: Option<String>,
    pub basis: AdmissionBasis,
    pub accepted_at: DateTime<Utc>,
    pub main_results: usize,
    pub lean_verified: usize,
}

/// A statement as readers see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicClaim {
    pub id: ClaimId,
    pub kind: ClaimKind,
    pub role: ClaimRole,
    pub label: String,
    pub statement: String,
    pub section: Option<String>,
    /// The checked Lean proof, once verified.
    pub lean: Option<FormalArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicVersion {
    pub number: u32,
    pub uploaded_at: DateTime<Utc>,
    pub has_pdf: bool,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicPaper {
    pub summary: PaperSummary,
    pub ai_disclosure: AiDisclosure,
    pub versions: Vec<PublicVersion>,
    pub claims: Vec<PublicClaim>,
    pub formalization_repository: Option<String>,
    /// Math macros of the current version, for rendering statements.
    pub macros: std::collections::BTreeMap<String, String>,
    /// Present when the author made the analysis public.
    pub analysis: Option<PaperAnalysis>,
    /// What became of the paper's conjectures, with the analysis.
    pub conjectures: Vec<ConjectureFollowUp>,
}

fn lean(plan: &FormalizationPlan, claim: &ClaimId) -> Option<FormalArtifact> {
    match plan.item(claim).map(|i| &i.state) {
        Some(ItemState::Verified { artifact, .. }) => Some(artifact.clone()),
        _ => None,
    }
}

fn summary(record: &Record, submission: &Submission) -> PaperSummary {
    PaperSummary {
        record: record.id.clone(),
        submission: submission.id.clone(),
        title: submission.title.clone(),
        authors: submission.authors.clone(),
        abstract_text: submission.abstract_text.clone(),
        msc: submission.msc.clone(),
        doi: submission.doi.clone(),
        basis: record.basis,
        accepted_at: record.accepted_at,
        main_results: submission
            .claims
            .iter()
            .filter(|c| c.is_main_result())
            .count(),
        lean_verified: submission.formalization.verified(),
    }
}

impl App {
    async fn accepted(&self, record: &Record) -> CoreResult<Submission> {
        let submission = self.load(&record.submission).await?;
        match &submission.status {
            SubmissionStatus::Accepted { record: r } if r == &record.id => Ok(submission),
            _ => Err(CoreError::not_found("paper", record.id.as_str())),
        }
    }

    /// Accepted papers, newest first.
    pub async fn list_papers(
        &self,
        limit: Option<u32>,
        before: Option<String>,
    ) -> CoreResult<Listing<PaperSummary>> {
        let page = self.ports.records.list(clamp(limit), before).await?;
        let mut items = Vec::with_capacity(page.items.len());
        for record in &page.items {
            if let Ok(submission) = self.accepted(record).await {
                items.push(summary(record, &submission));
            }
        }
        Ok(Listing {
            items,
            next_before: page.next_before,
        })
    }

    pub async fn paper(&self, id: &RecordId) -> CoreResult<PublicPaper> {
        let record = self
            .ports
            .records
            .get(id)
            .await?
            .ok_or_else(|| CoreError::not_found("paper", id.as_str()))?;
        let submission = self.accepted(&record).await?;
        let public = submission.analysis_visibility == Visibility::Public;
        let analysis = if public {
            let judgements = self.ports.judgements.for_submission(&submission.id).await?;
            Some(PaperAnalysis::compute(&submission, &judgements))
        } else {
            None
        };
        Ok(PublicPaper {
            summary: summary(&record, &submission),
            ai_disclosure: submission.ai_disclosure.clone(),
            versions: submission
                .versions
                .iter()
                .map(|v| PublicVersion {
                    number: v.number,
                    uploaded_at: v.uploaded_at,
                    has_pdf: v.pdf.is_some(),
                    note: v.note.clone(),
                })
                .collect(),
            claims: submission
                .claims
                .iter()
                .map(|c| PublicClaim {
                    id: c.id.clone(),
                    kind: c.kind,
                    role: c.role,
                    label: c.label.clone(),
                    statement: c.statement.clone(),
                    section: c.section.clone(),
                    lean: lean(&submission.formalization, &c.id),
                })
                .collect(),
            formalization_repository: submission.formalization.repository.clone(),
            macros: submission
                .current_version()
                .map(|v| v.macros.clone())
                .unwrap_or_default(),
            analysis,
            conjectures: if public {
                submission.conjectures.clone()
            } else {
                vec![]
            },
        })
    }
}
