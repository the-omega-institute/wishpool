//! What readers see: accepted papers, each statement with its badges, and
//! the analysis when the author made it public.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{App, clamp};
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, RecordId, SubmissionId},
    model::*,
    policy::AdmissionBasis,
    ports::Listing,
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PaperSummary {
    #[serde(default)]
    pub kind: SubmissionKind,
    #[serde(default)]
    pub public: bool,
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
    pub depends_on: Vec<ClaimId>,
    /// The checked Lean proof, once verified.
    pub lean: Option<FormalArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicVersion {
    pub number: u32,
    pub uploaded_at: DateTime<Utc>,
    pub has_pdf: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicPaper {
    pub summary: PaperSummary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<PublicVersion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<PublicClaim>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formalization_repository: Option<String>,
    /// Math macros of the current version, for rendering statements.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub macros: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new_content: Vec<PublicWitness>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lean_statements: Vec<PublicLeanStatement>,
}

/// Public mathematical propositions only; never comments or review rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicWitness {
    pub claim: ClaimId,
    pub lemmas: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicLeanStatement {
    pub claim: ClaimId,
    pub lean: String,
    pub digest: String,
    pub toolchain: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeanStatementStatus {
    None,
    AwaitingAuthor,
    Confirmed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConjectureSummary {
    pub record: RecordId,
    pub title: String,
    pub statement: String,
    pub lean_statement_status: LeanStatementStatus,
}

impl Serialize for PaperSummary {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serde_json::json!({"record": self.record, "title": self.title, "authors": self.authors, "kind": self.kind});
        if self.public {
            let fields = serde_json::json!({"submission": self.submission, "abstract_text": self.abstract_text,
                "msc": self.msc, "doi": self.doi, "basis": self.basis, "accepted_at": self.accepted_at,
                "main_results": self.main_results, "lean_verified": self.lean_verified});
            value
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
        }
        value.serialize(serializer)
    }
}

fn lean(plan: &FormalizationPlan, claim: &ClaimId) -> Option<FormalArtifact> {
    match plan.item(claim).map(|i| &i.state) {
        Some(ItemState::Verified { artifact, .. }) => Some(artifact.clone()),
        _ => None,
    }
}

fn summary(record: &Record, submission: &Submission) -> PaperSummary {
    PaperSummary {
        kind: submission.kind,
        public: submission.analysis_visibility == Visibility::Public,
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
        let current = self.load(&record.submission).await?;
        let current_version = current.current_version().map(|v| v.number);
        if let Some(publication) = record
            .publication
            .as_deref()
            .or_else(|| current.published_progress.as_ref()?.publication.as_deref())
        {
            let mut published = publication.clone();
            published.status = SubmissionStatus::Accepted {
                record: record.id.clone(),
            };
            published.analysis_visibility = current.analysis_visibility;
            // Later proofs can appear only for this same statement set.
            if current.claims_revision == published.claims_revision
                && current.current_version().map(|v| v.number)
                    == published.current_version().map(|v| v.number)
            {
                published.formalization = current.formalization;
                published.conjectures = current.conjectures;
                published.lean_statements = current.lean_statements;
            } else if let Some(progress) = current.published_progress
                && progress.claims_revision == published.claims_revision
                && Some(progress.version) == published.current_version().map(|v| v.number)
            {
                published.formalization = progress.formalization;
                published.conjectures = progress.conjectures;
            }
            if current_version != published.current_version().map(|v| v.number) {
                published.lean_statements.clear();
            }
            return Ok(published);
        }
        match &current.status {
            SubmissionStatus::Accepted { record: r } if r == &record.id => Ok(current),
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

    pub async fn list_conjectures(
        &self,
        limit: Option<u32>,
        before: Option<String>,
    ) -> CoreResult<Listing<ConjectureSummary>> {
        let limit = clamp(limit) as usize;
        let mut cursor = before;
        let mut items = vec![];
        loop {
            let page = self
                .ports
                .records
                .list((limit - items.len()) as u32, cursor)
                .await?;
            for record in &page.items {
                let Ok(s) = self.accepted(record).await else {
                    continue;
                };
                if s.kind != SubmissionKind::Conjecture
                    || s.analysis_visibility != Visibility::Public
                {
                    continue;
                }
                let Some(claim) = s
                    .claims
                    .iter()
                    .find(|c| c.kind.is_open() && c.role == ClaimRole::Main)
                else {
                    continue;
                };
                let status = s
                    .lean_statements
                    .iter()
                    .rev()
                    .find(|a| a.claim == claim.id)
                    .map_or(LeanStatementStatus::None, |a| match a.response {
                        LeanStatementResponse::Confirmed { .. } => LeanStatementStatus::Confirmed,
                        LeanStatementResponse::AwaitingAuthor => {
                            LeanStatementStatus::AwaitingAuthor
                        }
                        LeanStatementResponse::Rejected { .. } => LeanStatementStatus::None,
                    });
                items.push(ConjectureSummary {
                    record: record.id.clone(),
                    title: s.title,
                    statement: claim
                        .statement
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                    lean_statement_status: status,
                });
            }
            cursor = page.next_before;
            if items.len() == limit || cursor.is_none() {
                break;
            }
        }
        Ok(Listing {
            items,
            next_before: cursor,
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
        let new_content = if public {
            match submission.latest_report(Stage::Escape).map(|r| &r.payload) {
                Some(StagePayload::Escape { assessments }) => assessments
                    .iter()
                    .filter(|a| {
                        a.correctness == Some(Correctness::Correct)
                            && a.shape == ProofShape::Content
                            && submission
                                .claim(&a.claim)
                                .is_some_and(|c| c.is_main_result())
                    })
                    .map(|a| PublicWitness {
                        claim: a.claim.clone(),
                        lemmas: a.witnesses.clone(),
                    })
                    .collect(),
                _ => vec![],
            }
        } else {
            vec![]
        };
        let lean_statements = if public {
            submission
                .lean_statements
                .iter()
                .filter(|a| matches!(a.response, LeanStatementResponse::Confirmed { .. }))
                .map(|a| PublicLeanStatement {
                    claim: a.claim.clone(),
                    lean: a.lean.clone(),
                    digest: a.digest.clone(),
                    toolchain: a.toolchain.clone(),
                })
                .collect()
        } else {
            vec![]
        };
        Ok(PublicPaper {
            summary: summary(&record, &submission),
            versions: if public {
                submission
                    .versions
                    .iter()
                    .map(|v| PublicVersion {
                        number: v.number,
                        uploaded_at: v.uploaded_at,
                        has_pdf: v.pdf.is_some(),
                    })
                    .collect()
            } else {
                vec![]
            },
            claims: if public {
                submission
                    .claims
                    .iter()
                    .map(|c| PublicClaim {
                        id: c.id.clone(),
                        kind: c.kind,
                        role: c.role,
                        label: c.label.clone(),
                        statement: c.statement.clone(),
                        section: c.section.clone(),
                        depends_on: c.depends_on.clone(),
                        lean: lean(&submission.formalization, &c.id),
                    })
                    .collect()
            } else {
                vec![]
            },
            formalization_repository: public
                .then(|| submission.formalization.repository.clone())
                .flatten(),
            macros: if public {
                submission
                    .current_version()
                    .map(|v| v.macros.clone())
                    .unwrap_or_default()
            } else {
                Default::default()
            },
            new_content,
            lean_statements,
        })
    }
}
