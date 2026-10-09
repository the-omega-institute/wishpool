//! The analysis of a paper: stage reports filed by reviewers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{
    claim::{Claim, EscapeAssessment, validate_claims},
    common::{Source, require_text},
};
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, PersonId},
};

/// Analysis stages, in the order they must pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// The source compiles; AI use is disclosed.
    Hygiene,
    /// The author confirmed the statements and which are main results.
    Claims,
    /// Which statements are already in the literature.
    Literature,
    /// Which statements carry new content (escape witnesses).
    Escape,
}

impl Stage {
    pub const ALL: [Stage; 4] = [
        Stage::Hygiene,
        Stage::Claims,
        Stage::Literature,
        Stage::Escape,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Hygiene => "S0",
            Self::Claims => "S1",
            Self::Literature => "S2",
            Self::Escape => "S3",
        }
    }

    /// Stages whose report is computed against a specific claim set.
    pub fn depends_on_claims(self) -> bool {
        matches!(self, Self::Literature | Self::Escape)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum RejectReason {
    /// The source did not compile or its metadata is incomplete.
    Hygiene {
        detail: String,
    },
    /// Every main result with content is already in the literature.
    KnownResult {
        claim: ClaimId,
        prior: Source,
    },
    /// No main result carries new content.
    BindOnly,
    /// The paper states no proved main result.
    NoMainResult,
    OutOfScope {
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail {
        reason: RejectReason,
    },
    /// A human must decide. Machine reports on judgement stages land here.
    NeedsHuman {
        question: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewerIdentity {
    /// An editor (or the author, for the claims stage).
    Human { person: PersonId },
    Machine {
        account: PersonId,
        engine: String,
        model: Option<String>,
    },
}

impl ReviewerIdentity {
    pub fn is_human(&self) -> bool {
        matches!(self, Self::Human { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: String,
    pub locator: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorRelation {
    /// The prior work states the claim.
    Same,
    /// The prior work implies the claim directly.
    Implies,
    Related,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorWork {
    pub claim: ClaimId,
    pub source: Source,
    pub relation: PriorRelation,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HygieneCheck {
    pub name: String,
    pub passed: bool,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum StagePayload {
    Hygiene {
        checks: Vec<HygieneCheck>,
    },
    Claims {
        claims: Vec<Claim>,
    },
    Literature {
        prior: Vec<PriorWork>,
        searched: Vec<String>,
    },
    Escape {
        assessments: Vec<EscapeAssessment>,
    },
}

impl StagePayload {
    pub fn stage(&self) -> Stage {
        match self {
            Self::Hygiene { .. } => Stage::Hygiene,
            Self::Claims { .. } => Stage::Claims,
            Self::Literature { .. } => Stage::Literature,
            Self::Escape { .. } => Stage::Escape,
        }
    }
}

/// A filed report; append-only, the latest valid report per stage counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageReport {
    pub stage: Stage,
    pub outcome: Outcome,
    pub summary: String,
    pub payload: StagePayload,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    pub reviewer: ReviewerIdentity,
    pub claims_revision: u64,
    pub filed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportDraft {
    pub outcome: Outcome,
    pub summary: String,
    pub payload: StagePayload,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

impl ReportDraft {
    pub fn validate(&self, stage: Stage, claims: &[Claim]) -> CoreResult<()> {
        if self.payload.stage() != stage {
            return Err(CoreError::invalid(format!(
                "payload for {:?} filed on stage {:?}",
                self.payload.stage(),
                stage
            )));
        }
        require_text("summary", &self.summary, 20_000)?;
        if let Outcome::NeedsHuman { question } = &self.outcome {
            require_text("question", question, 5_000)?;
        }
        let known = |id: &ClaimId| claims.iter().any(|c| &c.id == id);
        match &self.payload {
            StagePayload::Hygiene { checks } => {
                if checks.is_empty() {
                    return Err(CoreError::invalid("hygiene report lists no checks"));
                }
                if checks.iter().any(|c| !c.passed) && self.outcome == Outcome::Pass {
                    return Err(CoreError::invalid(
                        "hygiene cannot pass with a failed check",
                    ));
                }
            }
            StagePayload::Claims { claims: proposed } => validate_claims(proposed)?,
            StagePayload::Literature { prior, searched } => {
                if claims.is_empty() {
                    return Err(CoreError::invalid(
                        "the literature is checked against confirmed claims",
                    ));
                }
                if searched.is_empty() {
                    return Err(CoreError::invalid(
                        "a literature report lists what was searched",
                    ));
                }
                for work in prior {
                    work.source.validate()?;
                    if !known(&work.claim) {
                        return Err(CoreError::invalid(format!("unknown claim {}", work.claim)));
                    }
                }
            }
            StagePayload::Escape { assessments } => {
                for assessment in assessments {
                    if !known(&assessment.claim) {
                        return Err(CoreError::invalid(format!(
                            "unknown claim {}",
                            assessment.claim
                        )));
                    }
                    assessment.validate()?;
                }
            }
        }
        Ok(())
    }
}
