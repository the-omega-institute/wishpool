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
    #[serde(default)]
    pub new_results: usize,
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
    pub kind: SubmissionKind,
    pub authors: Vec<Author>,
    pub accepted_at: DateTime<Utc>,
    pub new_results: usize,
    pub lean_verified: usize,
    pub claim: ClaimId,
    pub source: String,
    pub status: String,
    pub attempts: usize,
    pub solver: Option<Entrant>,
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
                "main_results": self.main_results, "new_results": self.new_results, "lean_verified": self.lean_verified});
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

/// Audited correct main content results, excluding any known result.
fn new_results(s: &Submission) -> usize {
    let analysis = crate::judgement::PaperAnalysis::compute(s, &[]);
    analysis
        .claims
        .iter()
        .filter(|c| {
            s.claim(&c.claim)
                .is_some_and(|claim| claim.is_main_result())
                && !c.known
                && c.assessment.as_ref().is_some_and(|a| {
                    a.correctness == Some(Correctness::Correct) && a.has_escape_content()
                })
        })
        .count()
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
        new_results: new_results(submission),
        lean_verified: submission.formalization.verified(),
    }
}

impl App {
    pub(crate) async fn accepted(&self, record: &Record) -> CoreResult<Submission> {
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
        self.list_conjectures_by_status(limit, before, None).await
    }
    pub async fn list_conjectures_by_status(
        &self,
        limit: Option<u32>,
        before: Option<String>,
        status: Option<String>,
    ) -> CoreResult<Listing<ConjectureSummary>> {
        if status
            .as_deref()
            .is_some_and(|s| !["open", "solved", "disproved"].contains(&s))
        {
            return Err(CoreError::invalid("invalid conjecture status"));
        }
        // The cursor addresses a claim, so multiple problems from one record
        // remain pageable without dropping the rest of the paper's claims.
        let mut items = vec![];
        let mut cursor = None;
        loop {
            let page = self.ports.records.list(100, cursor).await?;
            for record in page.items {
                let Ok(s) = self.conjecture_source(&record.id).await else {
                    continue;
                };
                if s.analysis_visibility != Visibility::Public {
                    continue;
                }
                for claim in s.claims.iter().filter(|c| c.kind.is_open()) {
                    if s.kind == SubmissionKind::Conjecture && claim.role != ClaimRole::Main {
                        continue;
                    }
                    let Ok(file) = self.solve_file(&record.id, &claim.id).await else {
                        continue;
                    };
                    if !file.admitted {
                        continue;
                    }
                    let key = format!("{}:{}", record.id, claim.id);
                    if before.as_ref().is_some_and(|b| &key >= b) {
                        continue;
                    }
                    let target = s.lean_statements.iter().rev().find(|a| {
                        a.claim == claim.id
                            && a.version == file.version
                            && a.claims_revision == file.claims_revision
                    });
                    let target_status =
                        target.map_or(LeanStatementStatus::None, |a| match a.response {
                            LeanStatementResponse::Confirmed { .. } => {
                                LeanStatementStatus::Confirmed
                            }
                            LeanStatementResponse::AwaitingAuthor => {
                                LeanStatementStatus::AwaitingAuthor
                            }
                            _ => LeanStatementStatus::None,
                        });
                    let winner = target
                        .filter(|_| target_status == LeanStatementStatus::Confirmed)
                        .and_then(|t| file.winner(&t.digest));
                    let solver = match winner {
                        Some(a) => Some(self.current_entrant(&a.entrant).await?),
                        None => None,
                    };
                    items.push(ConjectureSummary {
                        kind: s.kind,
                        authors: s.authors.clone(),
                        accepted_at: record.accepted_at,
                        new_results: new_results(&s),
                        lean_verified: file
                            .attempts
                            .iter()
                            .filter(|a| {
                                a.verified() && target.is_some_and(|t| t.digest == a.target_digest)
                            })
                            .count(),
                        record: record.id.clone(),
                        claim: claim.id.clone(),
                        title: if s.kind == SubmissionKind::Conjecture {
                            s.title.clone()
                        } else {
                            format!("{} — {}", s.title, claim.label)
                        },
                        statement: claim
                            .statement
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" "),
                        source: if s.kind == SubmissionKind::Conjecture {
                            "submitted".into()
                        } else {
                            format!("from {}", record.id)
                        },
                        status: winner
                            .map_or("open", |a| {
                                if a.receipt.as_ref().unwrap().verdict == Verdict::Proved {
                                    "solved"
                                } else {
                                    "disproved"
                                }
                            })
                            .into(),
                        attempts: file.attempts.len(),
                        solver,
                        lean_statement_status: target_status,
                    });
                }
            }
            cursor = page.next_before;
            if cursor.is_none() {
                break;
            }
        }
        items.retain(|c| status.as_ref().is_none_or(|s| s == &c.status));
        items.sort_by_key(|c| std::cmp::Reverse(format!("{}:{}", c.record, c.claim)));
        let limit = clamp(limit) as usize;
        let more = items.len() > limit;
        items.truncate(limit);
        let next_before = if more {
            items.last().map(|c| format!("{}:{}", c.record, c.claim))
        } else {
            None
        };
        Ok(Listing { items, next_before })
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

#[derive(Debug, Clone, Serialize)]
pub struct PublicConjecture {
    pub summary: ConjectureSummary,
    pub macros: std::collections::BTreeMap<String, String>,
    pub target: Option<PublicLeanStatement>,
    pub verified_attempts: Vec<PublicAttempt>,
}
impl App {
    pub async fn conjecture(
        &self,
        record: &RecordId,
        claim: &ClaimId,
    ) -> CoreResult<PublicConjecture> {
        let source = self.conjecture_source(record).await?;
        let mut cursor = None;
        let summary = loop {
            let page = self.list_conjectures(Some(100), cursor).await?;
            if let Some(s) = page
                .items
                .into_iter()
                .find(|s| &s.record == record && &s.claim == claim)
            {
                break s;
            }
            cursor = page.next_before;
            if cursor.is_none() {
                return Err(CoreError::not_found("conjecture", claim.as_str()));
            }
        };
        let target = match self.target(record, claim).await {
            Ok(t) => Some(t),
            Err(CoreError::Conflict(_)) => None,
            Err(e) => return Err(e),
        };
        let verified_attempts = if target.is_some() {
            self.public_attempts(record, claim).await?
        } else {
            vec![]
        };
        Ok(PublicConjecture {
            summary,
            macros: source
                .current_version()
                .map(|v| v.macros.clone())
                .unwrap_or_default(),
            target,
            verified_attempts,
        })
    }
}
