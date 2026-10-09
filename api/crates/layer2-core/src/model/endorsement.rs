use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::require_text;
use crate::{
    CoreResult,
    ids::{EndorsementId, PersonId, SubmissionId},
};

/// The endorser's own declaration of relationships with the authors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictDeclaration {
    /// Co-authored with any author in the last five years.
    pub recent_coauthor: bool,
    /// Same department or research group as any author.
    pub institutional_overlap: bool,
    /// Supervisor, student, or funding relationship with any author.
    pub advisory_or_funding: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

impl ConflictDeclaration {
    pub fn is_clear(&self) -> bool {
        !self.recent_coauthor
            && !self.institutional_overlap
            && !self.advisory_or_funding
            && self.other.as_deref().is_none_or(|o| o.trim().is_empty())
    }
}

/// A human attestation that the problem is genuine and the result matters in
/// the endorser's field. Machines never file endorsements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endorsement {
    pub id: EndorsementId,
    pub submission: SubmissionId,
    pub endorser: PersonId,
    pub significance: String,
    pub conflicts: ConflictDeclaration,
    /// Set by the service: the endorser is the submitter or a linked author.
    pub is_author: bool,
    pub filed_at: DateTime<Utc>,
}

impl Endorsement {
    pub fn is_independent(&self) -> bool {
        !self.is_author && self.conflicts.is_clear()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewEndorsement {
    pub significance: String,
    pub conflicts: ConflictDeclaration,
}

impl NewEndorsement {
    pub fn validate(&self) -> CoreResult<()> {
        require_text("significance", &self.significance, 5_000)
    }
}
