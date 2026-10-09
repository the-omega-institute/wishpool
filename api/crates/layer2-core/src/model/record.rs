use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::paper::Author;
use crate::{
    ids::{PersonId, RecordId, SubmissionId},
    policy::AdmissionBasis,
};

/// The acceptance of a paper. The public page is assembled from the
/// submission at read time, so formalizations added later appear on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// `WP-<year>-<4-digit sequence>`.
    pub id: RecordId,
    pub submission: SubmissionId,
    pub title: String,
    pub authors: Vec<Author>,
    pub basis: AdmissionBasis,
    pub accepted_by: PersonId,
    pub accepted_at: DateTime<Utc>,
}
