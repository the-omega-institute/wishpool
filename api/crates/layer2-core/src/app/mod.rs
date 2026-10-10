//! Application services: the complete set of operations any interface may
//! project. Authorization is decided here, never in an interface.

mod donations;
mod formalization;
mod lean_statements;
mod papers;
mod people;
mod records;
mod referee;
mod review;
mod tasks;

use std::{collections::BTreeSet, sync::Arc};

pub use papers::{MAX_UPLOAD_BYTES, PaperFile, SubmissionScope, Upload};
pub use records::{ConjectureSummary, PaperSummary, PublicClaim, PublicPaper};
pub use review::FiledBy;
pub use tasks::{MAX_ACTIVE_LEASES, Reconciliation, TaskContext, TaskGeneration};

use crate::{
    CoreError, CoreResult,
    ids::{PersonId, SubmissionId},
    model::{Caller, Role, Submission},
    policy::Policy,
    ports::Ports,
};

pub const MAX_PAGE: u32 = 100;

pub struct App {
    pub(crate) ports: Ports,
    pub(crate) policy: Policy,
    /// Subjects granted Admin on sign-in, from deployment configuration.
    pub(crate) bootstrap_admins: BTreeSet<PersonId>,
}

impl App {
    pub fn new(ports: Ports, policy: Policy, bootstrap_admins: BTreeSet<PersonId>) -> Arc<Self> {
        Arc::new(Self {
            ports,
            policy,
            bootstrap_admins,
        })
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// Editors, reviewers and admins see every paper under review.
    pub(crate) fn is_staff(caller: &Caller) -> bool {
        [Role::Editor, Role::Reviewer, Role::Admin]
            .iter()
            .any(|r| caller.has(*r))
    }

    pub(crate) async fn load(&self, id: &SubmissionId) -> CoreResult<Submission> {
        self.ports
            .submissions
            .get(id)
            .await?
            .ok_or_else(|| CoreError::not_found("paper", id.as_str()))
    }

    /// Store `submission` over the revision it was read at.
    pub(crate) async fn save(&self, submission: &mut Submission) -> CoreResult<()> {
        let expected = submission.revision;
        submission.updated_at = self.ports.clock.now();
        submission.revision = expected + 1;
        self.ports.submissions.replace(submission, expected).await
    }
}

pub(crate) fn clamp(limit: Option<u32>) -> u32 {
    limit.unwrap_or(25).clamp(1, MAX_PAGE)
}

#[cfg(test)]
mod tests;
