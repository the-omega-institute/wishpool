//! Private conjecture readings and author-confirmed, elaborated Lean targets.
use super::ProofShape;
use crate::ids::{ClaimId, PersonId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConjectureStatus {
    Open,
    KnownTrue,
    KnownFalse,
    SpecialCaseOfKnown,
    Unclear,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConjectureReading {
    pub well_posed: bool,
    pub well_posed_reason: String,
    pub status: ConjectureStatus,
    pub status_reason: String,
    /// Names reported by the referee/auditor, never verified bibliographic identifiers.
    pub named_works: Vec<String>,
    pub escape: ProofShape,
    pub escape_reason: String,
    pub suggestions: Vec<String>,
}
impl ConjectureReading {
    pub fn displayable(&self) -> bool {
        self.well_posed
            && self.status == ConjectureStatus::Open
            && self.escape == ProofShape::Content
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticationMethod {
    CookieSession,
    Bearer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LeanStatementResponse {
    AwaitingAuthor,
    Confirmed { author: PersonId, at: DateTime<Utc> },
    Rejected { comment: String, at: DateTime<Utc> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeanStatementAttempt {
    pub claim: ClaimId,
    pub version: u32,
    pub claims_revision: u64,
    pub lean: String,
    /// SHA-256 of the exact UTF-8 Lean source, checked in the binary.
    pub digest: String,
    pub toolchain: String,
    pub reading: String,
    pub response: LeanStatementResponse,
    pub created_at: DateTime<Utc>,
}
