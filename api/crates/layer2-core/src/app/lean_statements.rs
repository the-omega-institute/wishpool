//! The binary elaborates targets; the submitting author confirms their exact digest.
use super::App;
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, SubmissionId},
    model::*,
    ports::JobKind,
};
use sha2::{Digest, Sha256};

impl App {
    pub async fn queue_lean_statements(
        &self,
        caller: &Caller,
        id: &SubmissionId,
    ) -> CoreResult<()> {
        caller.require(Role::Reviewer)?;
        let s = self.submission(caller, id).await?;
        if s.kind != SubmissionKind::Conjecture
            || !matches!(s.status, SubmissionStatus::Accepted { .. })
        {
            return Ok(());
        }
        let file = self
            .ports
            .referees
            .get(id)
            .await?
            .ok_or_else(|| CoreError::conflict("no delivered letter"))?;
        let ready = file.current().is_some_and(|r| {
            r.version == s.current_version().unwrap().number
                && r.claims_revision == s.claims_revision
                && r.letter.done().is_some()
        });
        if !ready {
            return Err(CoreError::conflict(
                "Lean statements follow the delivered letter",
            ));
        }
        self.ports.queue.enqueue(id, JobKind::LeanStatement).await
    }

    /// Composition-only: a file independently elaborated by the configured Lean checker.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_lean_statement(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        version: u32,
        claims_revision: u64,
        claim: &ClaimId,
        lean: String,
        toolchain: String,
        reading: String,
        previous_digest: Option<String>,
    ) -> CoreResult<Submission> {
        caller.require(Role::Reviewer)?;
        let mut s = self.submission(caller, id).await?;
        if s.kind != SubmissionKind::Conjecture
            || !matches!(s.status, SubmissionStatus::Accepted { .. })
            || s.current_version().map(|v| v.number) != Some(version)
            || s.claims_revision != claims_revision
        {
            return Err(CoreError::conflict("the conjecture inputs changed"));
        }
        if !s
            .claim(claim)
            .is_some_and(|c| c.kind.is_open() && c.role == ClaimRole::Main)
        {
            return Err(CoreError::invalid(
                "target must name a confirmed main conjecture",
            ));
        }
        crate::model::require_text("Lean statement", &lean, 200_000)?;
        crate::model::require_text("toolchain", &toolchain, 2_000)?;
        crate::model::require_text("plain-language reading", &reading, 20_000)?;
        let latest = s.lean_statements.iter().rev().find(|a| &a.claim == claim);
        if latest.map(|a| a.digest.as_str()) != previous_digest.as_deref()
            || latest.is_some_and(|a| !matches!(a.response, LeanStatementResponse::Rejected { .. }))
        {
            return Err(CoreError::conflict("this target attempt was superseded"));
        }
        let file = self
            .ports
            .referees
            .get(id)
            .await?
            .ok_or_else(|| CoreError::conflict("no delivered letter"))?;
        if !file.current().is_some_and(|r| {
            r.version == version
                && r.claims_revision == claims_revision
                && r.letter.done().is_some()
        }) {
            return Err(CoreError::conflict(
                "Lean statements follow the delivered letter",
            ));
        }
        let digest = format!("{:x}", Sha256::digest(lean.as_bytes()));
        // A rejection must produce a different target; never re-confirm a rejected digest.
        if s.lean_statements
            .iter()
            .any(|a| a.claim == *claim && a.digest == digest)
        {
            return Err(CoreError::conflict("the regenerated target did not change"));
        }
        s.lean_statements.push(LeanStatementAttempt {
            claim: claim.clone(),
            version,
            claims_revision,
            lean,
            digest,
            toolchain,
            reading,
            response: LeanStatementResponse::AwaitingAuthor,
            created_at: self.ports.clock.now(),
        });
        self.save(&mut s).await?;
        Ok(s)
    }

    pub async fn respond_lean_statement(
        &self,
        caller: &Caller,
        authentication: AuthenticationMethod,
        id: &SubmissionId,
        digest: &str,
        confirm: bool,
        comment: String,
    ) -> CoreResult<Submission> {
        let mut s = self.load(id).await?;
        Self::require_submitter(caller, &s)?;
        if authentication != AuthenticationMethod::CookieSession {
            return Err(CoreError::forbidden(
                "confirm your conjecture in a signed-in browser session",
            ));
        }
        if s.kind != SubmissionKind::Conjecture
            || !matches!(s.status, SubmissionStatus::Accepted { .. })
        {
            return Err(CoreError::conflict(
                "only accepted conjectures have Lean statements to confirm",
            ));
        }
        let version = s.current_version().map(|v| v.number);
        let index = s
            .lean_statements
            .iter()
            .rposition(|a| a.digest == digest)
            .ok_or_else(|| CoreError::conflict("the Lean statement digest changed"))?;
        let attempt = &s.lean_statements[index];
        if Some(attempt.version) != version
            || attempt.claims_revision != s.claims_revision
            || !matches!(attempt.response, LeanStatementResponse::AwaitingAuthor)
            || s.lean_statements[index + 1..]
                .iter()
                .any(|a| a.claim == attempt.claim)
        {
            return Err(CoreError::conflict(
                "this Lean statement is no longer awaiting confirmation",
            ));
        }
        let response = if confirm {
            LeanStatementResponse::Confirmed {
                author: caller.person.clone(),
                at: self.ports.clock.now(),
            }
        } else {
            crate::model::require_text("correction", &comment, 10_000)?;
            LeanStatementResponse::Rejected {
                comment,
                at: self.ports.clock.now(),
            }
        };
        s.lean_statements[index].response = response;
        self.save(&mut s).await?;
        if !confirm {
            self.ports.queue.enqueue(id, JobKind::LeanStatement).await?;
        }
        Ok(s)
    }
}
