use std::collections::BTreeSet;

use super::App;
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, SubmissionId},
    model::{
        Caller, Correctness, EscapeAssessment, FeedbackLetter, MAX_LETTER_CHARS, NewLetter,
        Outcome, PriorRelation, PriorWork, ProofShape, RefereeFile, RefereeRound, RefereeView,
        ReviewerIdentity, Role, RoundUpdate, Source, SourceKind, Stage, StagePayload, StageReport,
        Step, StepState, Submission, SubmissionStatus,
    },
    ports::JobKind,
};

impl App {
    /// Staff see every round; the submitting author and co-authors see the
    /// reports, audits, sent letters and probe outcomes; advice/files stay private.
    pub async fn referee(&self, caller: &Caller, id: &SubmissionId) -> CoreResult<RefereeView> {
        let submission = self.submission(caller, id).await?;
        let file = self
            .ports
            .referees
            .get(id)
            .await?
            .unwrap_or_else(|| RefereeFile::new(submission.id.clone()));
        Ok(file.view(Self::is_staff(caller)))
    }

    /// The round for the paper's current version and statement set, started
    /// if there is none. Reviewer accounts only.
    pub async fn begin_referee_round(
        &self,
        caller: &Caller,
        id: &SubmissionId,
    ) -> CoreResult<RefereeFile> {
        caller.require(Role::Reviewer)?;
        let submission = self.submission(caller, id).await?;
        let (mut file, stored) = self.referee_file(id).await?;
        let version = Self::version_number(&submission)?;
        if file.current().is_some_and(|r| {
            r.version == version && r.claims_revision == submission.claims_revision
        }) {
            return Ok(file);
        }
        if !submission.is_open() {
            return Err(CoreError::conflict(format!(
                "the paper is {}",
                submission.status.name()
            )));
        }
        self.push_round(&mut file, &submission, version);
        self.store_referee_file(&mut file, stored).await?;
        Ok(file)
    }

    /// Start a new round on the current statements, e.g. after a failed
    /// step. Editors only.
    pub async fn restart_referee(
        &self,
        caller: &Caller,
        id: &SubmissionId,
    ) -> CoreResult<RefereeFile> {
        caller.require(Role::Editor)?;
        let submission = self.submission(caller, id).await?;
        if !submission.is_open() {
            return Err(CoreError::conflict(format!(
                "the paper is {}",
                submission.status.name()
            )));
        }
        let version = Self::version_number(&submission)?;
        let (mut file, stored) = self.referee_file(id).await?;
        if file.current().is_some_and(|r| !r.is_settled()) {
            return Err(CoreError::conflict("the current round is still running"));
        }
        self.push_round(&mut file, &submission, version);
        self.store_referee_file(&mut file, stored).await?;
        self.ports.queue.enqueue(id, JobKind::Referee).await?;
        Ok(file)
    }

    /// Record one step of round `number`. Reviewer accounts only. Results
    /// may name only statements of the round's statement set.
    pub async fn update_referee_round(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        number: u32,
        update: RoundUpdate,
    ) -> CoreResult<RefereeFile> {
        caller.require(Role::Reviewer)?;
        let submission = self.submission(caller, id).await?;
        let (mut file, stored) = self.referee_file(id).await?;
        let round = file
            .rounds
            .iter_mut()
            .find(|r| r.number == number)
            .ok_or_else(|| CoreError::not_found("referee round", number.to_string()))?;
        if round.version != Self::version_number(&submission)?
            || round.claims_revision != submission.claims_revision
        {
            return Err(CoreError::conflict(
                "the paper version or statements changed since the round started",
            ));
        }
        let known: BTreeSet<&ClaimId> = submission.claims.iter().map(|c| &c.id).collect();
        let check = |claims: Vec<&ClaimId>| -> CoreResult<()> {
            match claims.into_iter().find(|c| !known.contains(c)) {
                Some(unknown) => Err(CoreError::not_found("statement", unknown.as_str())),
                None => Ok(()),
            }
        };
        match update {
            RoundUpdate::Referee(step) => {
                ensure_mutable(&round.referee)?;
                if let Some(report) = step.done() {
                    check(
                        report
                            .claims
                            .iter()
                            .map(|c| &c.claim)
                            .chain(report.concerns.iter().filter_map(|c| c.claim.as_ref()))
                            .collect(),
                    )?;
                }
                round.referee = step;
            }
            RoundUpdate::Audit(step) => {
                ensure_mutable(&round.audit)?;
                if !matches!(step.state, StepState::Pending) && !round.referee.is_settled() {
                    return Err(CoreError::conflict("audit must wait for the referee"));
                }
                if let Some(audit) = step.done() {
                    if round.referee.done().is_none() {
                        return Err(CoreError::conflict(
                            "audit requires a completed referee report",
                        ));
                    }
                    check(audit.claims.iter().map(|c| &c.claim).collect())?;
                    let ids: BTreeSet<_> = audit.claims.iter().map(|c| &c.claim).collect();
                    if ids.len() != submission.claims.len() || ids.len() != audit.claims.len() {
                        return Err(CoreError::invalid(
                            "audit must cover every confirmed statement exactly once",
                        ));
                    }
                }
                round.audit = step;
            }
            RoundUpdate::Advice(step) => {
                ensure_mutable(&round.advice)?;
                if !matches!(step.state, StepState::Pending) {
                    if !round.audit.is_settled() {
                        return Err(CoreError::conflict("advice must wait for the audit"));
                    }
                    if !matches!(step.state, StepState::Skipped { .. })
                        && (round.audit.done().is_none()
                            || !matches!(
                                submission.status,
                                SubmissionStatus::Accepted { .. } | SubmissionStatus::NotAccepted
                            ))
                    {
                        return Err(CoreError::conflict(
                            "advice must wait for the audited decision",
                        ));
                    }
                }
                if let Some(advice) = step.done() {
                    check(
                        advice
                            .improvements
                            .iter()
                            .filter_map(|i| i.claim.as_ref())
                            .chain(advice.formalization.iter().map(|f| &f.claim))
                            .collect(),
                    )?;
                }
                round.advice = step;
            }
            RoundUpdate::Formal(step) => {
                ensure_mutable(&round.formal)?;
                if !matches!(step.state, StepState::Pending | StepState::Skipped { .. }) {
                    if !matches!(submission.status, SubmissionStatus::Accepted { .. })
                        || round.letter.done().is_none()
                    {
                        return Err(CoreError::conflict(
                            "formalization requires acceptance and a delivered letter",
                        ));
                    }
                    let candidates: BTreeSet<&ClaimId> = round
                        .advice
                        .done()
                        .map(|a| a.formalization.iter().map(|f| &f.claim).collect())
                        .unwrap_or_default();
                    if candidates.is_empty() {
                        return Err(CoreError::conflict(
                            "formalization needs advice with formalization candidates; skip it otherwise",
                        ));
                    }
                    if let Some(other) = step.done().and_then(|probe| {
                        probe
                            .attempts
                            .iter()
                            .find(|a| !candidates.contains(&a.claim))
                    }) {
                        return Err(CoreError::conflict(format!(
                            "{} was not proposed for formalization",
                            other.claim
                        )));
                    }
                }
                if !matches!(step.state, StepState::Pending)
                    && (!round.advice.is_settled() || !round.letter.is_settled())
                {
                    return Err(CoreError::conflict(
                        "formalization must wait for the advice and letter",
                    ));
                }
                round.formal = step;
            }
            RoundUpdate::Letter(mut step) => {
                ensure_mutable(&round.letter)?;
                if !matches!(step.state, StepState::Pending)
                    && (!round.referee.is_settled()
                        || !round.audit.is_settled()
                        || !round.advice.is_settled())
                {
                    return Err(CoreError::conflict(
                        "the letter must wait for the referee, audit and advice",
                    ));
                }
                if let Some(letter) = step.done() {
                    if round.audit.done().is_none()
                        || !matches!(
                            submission.status,
                            SubmissionStatus::Accepted { .. } | SubmissionStatus::NotAccepted
                        )
                    {
                        return Err(CoreError::conflict(
                            "the letter requires the audited decision",
                        ));
                    }
                    if round.referee.done().is_none() {
                        return Err(CoreError::conflict(
                            "a letter draft requires a completed referee report",
                        ));
                    }
                    validate_letter(&letter.subject, &letter.body, &letter.note)?;
                }
                if let StepState::Done { result: draft, .. } = &mut step.state {
                    let first = match &submission.status {
                        SubmissionStatus::Accepted { record } => {
                            if submission.kind == crate::model::SubmissionKind::Conjecture {
                                format!("Your conjecture is displayed as {record}.")
                            } else {
                                format!("Your paper is accepted as {record}.")
                            }
                        }
                        SubmissionStatus::NotAccepted => {
                            let reasons = match &submission.decision {
                                Some(crate::policy::Decision::NotAccepted { reasons }) => reasons
                                    .iter()
                                    .map(|r| r.author_text())
                                    .collect::<Vec<_>>()
                                    .join(" "),
                                _ => String::new(),
                            };
                            if submission.kind == crate::model::SubmissionKind::Conjecture {
                                format!("Your conjecture is not displayed. {reasons}")
                            } else {
                                format!("Your paper is not accepted. {reasons}")
                            }
                        }
                        _ => return Err(CoreError::conflict("the decision has not been applied")),
                    };
                    draft.body = with_decision(&draft.body, &first);
                    validate_letter(&draft.subject, &draft.body, &draft.note)?;
                }
                if let Some(draft) = step.done() {
                    file.letters.push(FeedbackLetter {
                        round: Some(number),
                        assessment: round.audit.done().map(|a| a.verdict),
                        subject: draft.subject.clone(),
                        body: draft.body.clone(),
                        note: draft.note.clone(),
                        edited: false,
                        sent_by: caller.person.clone(),
                        sent_at: self.ports.clock.now(),
                    });
                }
                round.letter = step;
            }
        }
        self.store_referee_file(&mut file, stored).await?;
        Ok(file)
    }

    /// Replay-safe settlement of a persisted audit. Both reports are appended
    /// together under the submission revision fence; record insertion can be recovered.
    pub async fn apply_referee_audit(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        number: u32,
    ) -> CoreResult<Submission> {
        caller.require(Role::Reviewer)?;
        let mut submission = self.submission(caller, id).await?;
        if submission.involves(&caller.person) {
            return Err(CoreError::forbidden("authors cannot audit their own paper"));
        }
        let (file, _) = self.referee_file(id).await?;
        let round = file
            .current()
            .filter(|r| r.number == number)
            .ok_or_else(|| CoreError::conflict("only the current audit can decide"))?;
        if round.version != Self::version_number(&submission)?
            || round.claims_revision != submission.claims_revision
        {
            return Err(CoreError::conflict("audit inputs changed"));
        }
        let audit = round
            .audit
            .done()
            .ok_or_else(|| CoreError::conflict("audit is not complete"))?;
        if matches!(
            submission.status,
            SubmissionStatus::Accepted { .. } | SubmissionStatus::NotAccepted
        ) {
            return Ok(submission);
        }
        if !submission.is_open() {
            return Err(CoreError::conflict("paper is not in review"));
        }
        let marker = format!(
            "referee-audit:{}:r{number}:v{}:c{}",
            id, round.version, round.claims_revision
        );
        let filed = [Stage::Literature, Stage::Escape].iter().all(|stage| {
            submission.latest_report(*stage).is_some_and(|r| {
                r.evidence
                    .iter()
                    .any(|e| e.kind == "referee_audit" && e.locator == marker)
            })
        });
        if !filed {
            let prior = audit
                .claims
                .iter()
                .filter_map(|c| {
                    c.known.as_ref().map(|known| PriorWork {
                        claim: c.claim.clone(),
                        source: Source {
                            kind: SourceKind::NamedWork,
                            locator: known.clone(),
                            year: None,
                        },
                        relation: PriorRelation::Same,
                        note: c.comment.clone(),
                    })
                })
                .collect();
            let assessments = submission
                .claims
                .iter()
                .filter(|c| {
                    c.is_main_result()
                        || (submission.kind == crate::model::SubmissionKind::Conjecture
                            && c.role == crate::model::ClaimRole::Main
                            && c.kind.is_open())
                })
                .map(|c| {
                    let reading = audit
                        .claims
                        .iter()
                        .find(|a| a.claim == c.id)
                        .expect("validated complete audit");
                    let carries = reading.correctness == Correctness::Correct
                        && reading.shape == Some(ProofShape::Content)
                        && !reading.witnesses.is_empty();
                    EscapeAssessment {
                        conjecture: reading.conjecture.clone(),
                        correctness: Some(reading.correctness),
                        claim: c.id.clone(),
                        shape: if let Some(r) = &reading.conjecture {
                            r.escape
                        } else if carries {
                            ProofShape::Content
                        } else {
                            ProofShape::BindOnly
                        },
                        witnesses: if carries {
                            reading.witnesses.clone()
                        } else {
                            vec![]
                        },
                        rationale: format!("{:?}: {}", reading.correctness, reading.comment),
                        escape_rate: None,
                    }
                })
                .collect();
            for (stage, payload) in [
                (
                    Stage::Literature,
                    StagePayload::Literature {
                        prior,
                        searched: vec![
                            "Paper source and works named in the referee report; offline audit."
                                .into(),
                        ],
                    },
                ),
                (Stage::Escape, StagePayload::Escape { assessments }),
            ] {
                let draft = crate::model::ReportDraft {
                    outcome: Outcome::Pass,
                    summary: audit.summary.clone(),
                    payload,
                    evidence: vec![],
                };
                draft.validate(stage, &submission.claims)?;
                submission.push_report(StageReport {
                    stage,
                    outcome: draft.outcome,
                    summary: draft.summary,
                    payload: draft.payload,
                    evidence: vec![crate::model::Evidence {
                        kind: "referee_audit".into(),
                        locator: marker.clone(),
                        note: "Source-based audit; not a Lean verification.".into(),
                    }],
                    reviewer: ReviewerIdentity::Machine {
                        account: caller.person.clone(),
                        engine: "codex-cli".into(),
                        model: round.audit.model.clone(),
                    },
                    claims_revision: round.claims_revision,
                    filed_at: self.ports.clock.now(),
                });
            }
            self.save(&mut submission).await?;
        }
        self.apply_decision(caller, submission).await
    }

    /// Send feedback to the author. Editors only; the body is what the
    /// author reads, usually the edited draft of the current round.
    pub async fn send_feedback(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        new: NewLetter,
    ) -> CoreResult<FeedbackLetter> {
        caller.require(Role::Editor)?;
        let submission = self.submission(caller, id).await?;
        if submission.status == SubmissionStatus::Withdrawn {
            return Err(CoreError::conflict("the paper is withdrawn"));
        }
        validate_letter(&new.subject, &new.body, &new.note)?;
        let (mut file, stored) = self.referee_file(id).await?;
        let letter = FeedbackLetter {
            round: file.current().map(|r| r.number),
            assessment: new.assessment,
            edited: file
                .current()
                .and_then(|r| r.letter.done())
                .is_none_or(|draft| {
                    draft.subject != new.subject || draft.body != new.body || draft.note != new.note
                }),
            subject: new.subject,
            body: new.body,
            note: new.note,
            sent_by: caller.person.clone(),
            sent_at: self.ports.clock.now(),
        };
        file.letters.push(letter.clone());
        self.store_referee_file(&mut file, stored).await?;
        Ok(letter)
    }

    fn version_number(submission: &Submission) -> CoreResult<u32> {
        submission
            .current_version()
            .map(|v| v.number)
            .ok_or_else(|| CoreError::conflict("the paper has no version"))
    }

    fn push_round(&self, file: &mut RefereeFile, submission: &Submission, version: u32) {
        let number = file.rounds.last().map_or(1, |r| r.number + 1);
        file.rounds.push(RefereeRound {
            number,
            version,
            claims_revision: submission.claims_revision,
            started_at: self.ports.clock.now(),
            referee: Step::pending(),
            audit: Step::pending(),
            advice: Step::pending(),
            formal: Step::pending(),
            letter: Step::pending(),
        });
    }

    /// The stored file (and whether it exists), or a new one.
    async fn referee_file(&self, id: &SubmissionId) -> CoreResult<(RefereeFile, bool)> {
        Ok(match self.ports.referees.get(id).await? {
            Some(file) => (file, true),
            None => (RefereeFile::new(id.clone()), false),
        })
    }

    async fn store_referee_file(&self, file: &mut RefereeFile, stored: bool) -> CoreResult<()> {
        if stored {
            let expected = file.revision;
            self.ports.referees.replace(file, expected).await?;
            file.revision = expected + 1;
        } else {
            self.ports.referees.insert(file).await?;
        }
        Ok(())
    }
}

fn ensure_mutable<T>(step: &Step<T>) -> CoreResult<()> {
    if step.is_settled() {
        return Err(CoreError::conflict(
            "a settled step is immutable; restart the referee round",
        ));
    }
    Ok(())
}

/// Puts the decision sentence after the salutation ("Dear ...,"), or first when there is none.
fn with_decision(body: &str, decision: &str) -> String {
    let body = body.trim_start();
    match body.split_once('\n') {
        Some((salutation, rest)) if salutation.starts_with("Dear ") => {
            format!("{salutation}\n\n{decision}\n\n{}", rest.trim_start())
        }
        _ => format!("{decision}\n\n{body}"),
    }
}

fn validate_letter(subject: &str, body: &str, note: &str) -> CoreResult<()> {
    if body.trim().is_empty() {
        return Err(CoreError::invalid("the letter is empty"));
    }
    if subject.chars().count() > 300 {
        return Err(CoreError::invalid("the subject is too long"));
    }
    if body.chars().count() > MAX_LETTER_CHARS || note.chars().count() > MAX_LETTER_CHARS {
        return Err(CoreError::invalid("the letter is too long"));
    }
    Ok(())
}

#[cfg(test)]
mod letter_tests {
    use super::with_decision;

    #[test]
    fn decision_follows_the_salutation() {
        assert_eq!(
            with_decision(
                "Dear A. Author,\n\nThank you.",
                "Your paper is accepted as WP-2026-0001."
            ),
            "Dear A. Author,\n\nYour paper is accepted as WP-2026-0001.\n\nThank you."
        );
        assert_eq!(
            with_decision("Thank you.", "Your paper is not accepted."),
            "Your paper is not accepted.\n\nThank you."
        );
    }
}
