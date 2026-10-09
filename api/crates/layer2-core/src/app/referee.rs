//! Referee rounds and feedback letters. Machine accounts run the steps of a
//! round; editors restart rounds and send letters; the author reads only
//! the letters that were sent.

use std::collections::BTreeSet;

use super::App;
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, SubmissionId},
    model::{
        Caller, FeedbackLetter, MAX_LETTER_CHARS, NewLetter, RefereeFile, RefereeRound, Role,
        RoundUpdate, Step, StepState, Submission, SubmissionStatus,
    },
    ports::JobKind,
};

impl App {
    /// Staff see every round; the submitting author and co-authors see the
    /// letters that were sent.
    pub async fn referee(&self, caller: &Caller, id: &SubmissionId) -> CoreResult<RefereeFile> {
        let submission = self.submission(caller, id).await?;
        let mut file = self
            .ports
            .referees
            .get(id)
            .await?
            .unwrap_or_else(|| RefereeFile::new(submission.id.clone()));
        if !Self::is_staff(caller) {
            file.rounds.clear();
        }
        Ok(file)
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
            RoundUpdate::Advice(step) => {
                ensure_mutable(&round.advice)?;
                if !matches!(step.state, StepState::Pending) {
                    if !round.referee.is_settled() {
                        return Err(CoreError::conflict("advice must wait for the referee"));
                    }
                    let positive = round
                        .referee
                        .done()
                        .and_then(|r| r.recommendation)
                        .is_some_and(|r| r.is_positive());
                    if !positive && !matches!(step.state, StepState::Skipped { .. }) {
                        return Err(CoreError::conflict(
                            "advice requires a positive referee recommendation; skip it otherwise",
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
                if !matches!(step.state, StepState::Pending) && !round.advice.is_settled() {
                    return Err(CoreError::conflict(
                        "formalization must wait for the advice",
                    ));
                }
                round.formal = step;
            }
            RoundUpdate::Letter(step) => {
                ensure_mutable(&round.letter)?;
                if !matches!(step.state, StepState::Pending)
                    && (!round.referee.is_settled()
                        || !round.advice.is_settled()
                        || !round.formal.is_settled())
                {
                    return Err(CoreError::conflict(
                        "the letter must wait for the referee, advice and formalization",
                    ));
                }
                if let Some(letter) = step.done() {
                    if round.referee.done().is_none() {
                        return Err(CoreError::conflict(
                            "a letter draft requires a completed referee report",
                        ));
                    }
                    validate_letter(&letter.subject, &letter.body, &letter.note)?;
                }
                round.letter = step;
            }
        }
        self.store_referee_file(&mut file, stored).await?;
        Ok(file)
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
