//! After acceptance: formalizing the statements worth it, with the author's
//! consent, and following up the paper's conjectures.

use super::App;
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, SubmissionId},
    model::*,
};

impl App {
    fn require_accepted(submission: &Submission) -> CoreResult<()> {
        if matches!(submission.status, SubmissionStatus::Accepted { .. }) {
            Ok(())
        } else {
            Err(CoreError::conflict("this follows acceptance"))
        }
    }

    /// An editor proposes formalizing a proved statement.
    pub async fn propose_formalization(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        claim: &ClaimId,
        reason: String,
    ) -> CoreResult<Submission> {
        caller.require(Role::Editor)?;
        validate_reason(&reason)?;
        let mut submission = self.submission(caller, id).await?;
        Self::require_accepted(&submission)?;
        let statement = submission
            .claim(claim)
            .ok_or_else(|| CoreError::not_found("statement", claim.as_str()))?;
        if statement.kind.is_open() {
            return Err(CoreError::invalid(
                "conjectures are followed up, not formalized as proved results",
            ));
        }
        if submission.formalization.item(claim).is_some() {
            return Err(CoreError::conflict("already in the formalization plan"));
        }
        let now = self.ports.clock.now();
        submission.formalization.items.push(FormalizationItem {
            claim: claim.clone(),
            reason,
            state: ItemState::Proposed,
            updated_at: now,
        });
        self.save(&mut submission).await?;
        Ok(submission)
    }

    /// The author approves or declines a proposed formalization. Approval
    /// opens a formalization task when the author lets contributors help.
    pub async fn respond_to_formalization(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        claim: &ClaimId,
        approve: bool,
        reason: String,
    ) -> CoreResult<Submission> {
        let mut submission = self.load(id).await?;
        Self::require_submitter(caller, &submission)?;
        let now = self.ports.clock.now();
        let item = submission
            .formalization
            .items
            .iter_mut()
            .find(|i| &i.claim == claim)
            .ok_or_else(|| CoreError::not_found("plan item", claim.as_str()))?;
        if item.state != ItemState::Proposed {
            return Err(CoreError::conflict(format!(
                "the item is {}",
                item.state.name()
            )));
        }
        item.state = if approve {
            ItemState::Approved
        } else {
            ItemState::Declined {
                reason: reason.chars().take(2_000).collect(),
            }
        };
        item.updated_at = now;
        let open = submission.open_to_contributors;
        self.save(&mut submission).await?;
        if approve && open {
            self.open_task(caller, id, claim, TaskKind::Formalize)
                .await?;
        }
        Ok(submission)
    }

    pub async fn set_formalization_repository(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        repository: String,
    ) -> CoreResult<Submission> {
        caller.require(Role::Editor)?;
        if !is_web_url(&repository) {
            return Err(CoreError::invalid("the repository must be an http(s) URL"));
        }
        let mut submission = self.submission(caller, id).await?;
        Self::require_accepted(&submission)?;
        submission.formalization.repository = Some(repository);
        self.save(&mut submission).await?;
        Ok(submission)
    }

    pub async fn start_formalization(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        claim: &ClaimId,
    ) -> CoreResult<Submission> {
        caller.require(Role::Editor)?;
        let mut submission = self.submission(caller, id).await?;
        let now = self.ports.clock.now();
        let item = submission
            .formalization
            .items
            .iter_mut()
            .find(|i| &i.claim == claim)
            .ok_or_else(|| CoreError::not_found("plan item", claim.as_str()))?;
        if item.state != ItemState::Approved {
            return Err(CoreError::conflict(format!(
                "the item is {}; the author approves first",
                item.state.name()
            )));
        }
        item.state = ItemState::InProgress;
        item.updated_at = now;
        self.save(&mut submission).await?;
        Ok(submission)
    }

    /// An editor records a checked Lean proof. Only the standard axioms are
    /// accepted. A contribution named here is credited.
    pub async fn verify_formalization(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        claim: &ClaimId,
        artifact: FormalArtifact,
        axioms: Vec<String>,
        contribution: Option<String>,
    ) -> CoreResult<Submission> {
        caller.require(Role::Editor)?;
        validate_verification(&artifact, &axioms)?;
        let mut submission = self.submission(caller, id).await?;
        Self::require_accepted(&submission)?;
        let now = self.ports.clock.now();
        let item = submission
            .formalization
            .items
            .iter_mut()
            .find(|i| &i.claim == claim)
            .ok_or_else(|| CoreError::not_found("plan item", claim.as_str()))?;
        if !matches!(item.state, ItemState::Approved | ItemState::InProgress) {
            return Err(CoreError::conflict(format!(
                "the item is {}",
                item.state.name()
            )));
        }
        item.state = ItemState::Verified {
            artifact: artifact.clone(),
            axioms,
            contribution: contribution.clone(),
        };
        item.updated_at = now;
        self.save(&mut submission).await?;
        if let Some(contribution) = contribution {
            self.credit_formalization(&contribution, id, claim, &artifact)
                .await?;
        }
        Ok(submission)
    }

    /// An editor records what is happening with a conjecture of the paper.
    /// Taking it up opens a probe task when the author lets contributors help.
    pub async fn update_conjecture(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        claim: &ClaimId,
        state: ConjectureState,
    ) -> CoreResult<Submission> {
        caller.require(Role::Editor)?;
        let mut submission = self.submission(caller, id).await?;
        Self::require_accepted(&submission)?;
        if !submission.claim(claim).is_some_and(|c| c.kind.is_open()) {
            return Err(CoreError::invalid(
                "only the paper's conjectures and questions are followed up",
            ));
        }
        match &state {
            ConjectureState::NotPursued { reason } => validate_reason(reason)?,
            ConjectureState::Settled { summary, .. } => validate_reason(summary)?,
            _ => {}
        }
        let now = self.ports.clock.now();
        let taken_up = state == ConjectureState::TakenUp;
        match submission
            .conjectures
            .iter_mut()
            .find(|c| &c.claim == claim)
        {
            Some(follow_up) => {
                follow_up.state = state;
                follow_up.updated_at = now;
            }
            None => submission.conjectures.push(ConjectureFollowUp {
                claim: claim.clone(),
                state,
                updated_at: now,
            }),
        }
        let open = submission.open_to_contributors;
        self.save(&mut submission).await?;
        if taken_up && open {
            self.open_task(caller, id, claim, TaskKind::Probe).await?;
        }
        Ok(submission)
    }
}
