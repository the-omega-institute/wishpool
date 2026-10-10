//! Resume one persisted round, releasing the job lease between external polls
//! and between advisor calls.

use sha2::{Digest, Sha256};
use wishpool_core::{
    CoreError,
    app::FiledBy,
    model::{RoundUpdate, StepState, Submission, SubmissionStatus},
};
use wishpool_review::{
    Document, ReviewError,
    advisor::AdvisorInput,
    oracle::{OracleRequest, OracleStatus},
    referee_prompts::{self, RefereeOut, parse_answer},
};

use super::{
    LeasedJob, mapping,
    worker::{Failure, Worker},
};

fn transient(error: &ReviewError) -> bool {
    matches!(error, ReviewError::Transport(_))
        || matches!(error, ReviewError::Provider { status, .. } if *status == 429 || *status >= 500)
}

fn retryable(reason: &str) -> bool {
    matches!(
        reason,
        "infrastructure_retry_exhausted"
            | "prompt_delivery_uncertain"
            | "usage_limit_reached"
            | "model_unavailable"
    )
}

/// Length-prefix parts so the digest binds their boundaries as well as bytes.
fn input_digest(prompt: &str, input: &[u8]) -> String {
    let mut hash = Sha256::new();
    for part in [prompt.as_bytes(), input] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}

impl Worker {
    pub(super) async fn referee_job(&self, job: &LeasedJob) -> Result<bool, Failure> {
        if self.oracle.is_none() {
            return Ok(false);
        }
        let submission = match self
            .app
            .submission(&self.referee_account, &job.submission)
            .await
        {
            Ok(paper) => paper,
            Err(CoreError::NotFound { .. }) => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        let result = self.resume_referee(job, &submission).await;
        if result.is_err() {
            let current = self
                .app
                .submission(&self.referee_account, &job.submission)
                .await?;
            if current.claims_revision != submission.claims_revision
                || current.current_version().map(|v| v.number)
                    != submission.current_version().map(|v| v.number)
            {
                tracing::info!(
                    "referee inputs superseded; resume the job on the current confirmed paper"
                );
                return self.defer_referee(job).await;
            }
        }
        result
    }

    async fn resume_referee(
        &self,
        job: &LeasedJob,
        submission: &Submission,
    ) -> Result<bool, Failure> {
        let Some(oracle) = &self.oracle else {
            return Ok(false);
        };
        if matches!(
            submission.status,
            SubmissionStatus::Withdrawn | SubmissionStatus::Draft
        ) {
            return Ok(false);
        }
        self.ensure_lease(job).await?;
        let file = self
            .app
            .begin_referee_round(&self.referee_account, &submission.id)
            .await?;
        let Some(round) = file.current() else {
            return Ok(false);
        };
        if round.is_settled() {
            return Ok(false);
        }
        let number = round.number;
        let mut step = round.referee.clone();
        match &step.state {
            StepState::Pending => {
                if let Some(version) = submission
                    .versions
                    .iter()
                    .find(|version| version.number == round.version)
                {
                    if version.compile_error.is_some() {
                        step.state = self.failed("the paper did not compile".into(), None, false);
                        self.ensure_lease(job).await?;
                        self.app
                            .update_referee_round(
                                &self.referee_account,
                                &submission.id,
                                number,
                                RoundUpdate::Referee(step),
                            )
                            .await?;
                        return self.defer_referee(job).await;
                    }
                    if version.pdf.is_none() {
                        return self.defer_referee(job).await;
                    }
                }
                let pdf = match self
                    .app
                    .paper_file(
                        Some(&self.referee_account),
                        &submission.id,
                        Some(round.version),
                        true,
                    )
                    .await
                {
                    Ok(file) => file,
                    Err(CoreError::NotFound { .. }) => {
                        step.state =
                            self.failed("the paper has no compiled PDF".into(), None, false);
                        self.ensure_lease(job).await?;
                        self.app
                            .update_referee_round(
                                &self.referee_account,
                                &submission.id,
                                number,
                                RoundUpdate::Referee(step),
                            )
                            .await?;
                        return self.defer_referee(job).await;
                    }
                    Err(error) => return Err(error.into()),
                };
                let make_prompt =
                    if submission.kind == wishpool_core::model::SubmissionKind::Conjecture {
                        referee_prompts::conjecture_referee
                    } else {
                        referee_prompts::referee
                    };
                let prompt = make_prompt(
                    &Document {
                        title: submission.title.clone(),
                        abstract_text: submission.abstract_text.clone(),
                        text: None,
                    },
                    &mapping::confirmed_statements(&submission.claims),
                );
                let client_ref = format!(
                    "wishpool:{}:r{number}:v{}:c{}",
                    submission.id, round.version, round.claims_revision
                );
                let digest = input_digest(&prompt, &pdf.bytes);
                if step.input_digest.as_ref().is_some_and(|old| old != &digest)
                    || step
                        .model
                        .as_ref()
                        .is_some_and(|old| old != &self.referee_model)
                {
                    step.state = self.failed(
                        "referee inputs or model changed after admission".into(),
                        None,
                        false,
                    );
                    self.ensure_lease(job).await?;
                    self.app
                        .update_referee_round(
                            &self.referee_account,
                            &submission.id,
                            number,
                            RoundUpdate::Referee(step),
                        )
                        .await?;
                    return self.defer_referee(job).await;
                }
                // Persist the idempotency identity before crossing the transport boundary.
                step.engine = Some(oracle.engine().into());
                step.model = Some(self.referee_model.clone());
                step.client_ref = Some(client_ref.clone());
                step.input_digest = Some(digest);
                self.ensure_lease(job).await?;
                self.app
                    .update_referee_round(
                        &self.referee_account,
                        &submission.id,
                        number,
                        RoundUpdate::Referee(step.clone()),
                    )
                    .await?;
                let request = OracleRequest {
                    prompt,
                    pdf: Some((pdf.filename, pdf.bytes)),
                    client_ref,
                    tag: format!("wishpool:{}:r{number}", submission.id),
                };
                match oracle.submit(&request).await {
                    Ok(submitted) => {
                        step.attempts += 1;
                        step.state = StepState::Running {
                            task: Some(submitted.task),
                            queue_position: submitted.queue_position,
                            since: chrono::Utc::now(),
                        };
                    }
                    Err(error) if transient(&error) => {
                        tracing::warn!(%error, "oracle submission will be resumed with its client reference");
                        return self.defer_referee(job).await;
                    }
                    Err(error) => {
                        step.attempts += 1;
                        step.state = self.failed(error.to_string(), None, false);
                    }
                }
                self.ensure_lease(job).await?;
                self.app
                    .update_referee_round(
                        &self.referee_account,
                        &submission.id,
                        number,
                        RoundUpdate::Referee(step),
                    )
                    .await?;
                return self.defer_referee(job).await;
            }
            StepState::Running {
                task,
                queue_position,
                since,
            } => {
                let Some(task) = task else {
                    step.state =
                        self.failed("oracle running step has no task id".into(), None, false);
                    self.ensure_lease(job).await?;
                    self.app
                        .update_referee_round(
                            &self.referee_account,
                            &submission.id,
                            number,
                            RoundUpdate::Referee(step),
                        )
                        .await?;
                    return self.defer_referee(job).await;
                };
                let status = match oracle.poll(task).await {
                    Ok(status) => status,
                    Err(error) if transient(&error) => {
                        tracing::warn!(%error, "oracle poll will be retried");
                        return self.defer_referee(job).await;
                    }
                    Err(error) => {
                        step.state = self.failed(error.to_string(), None, false);
                        self.ensure_lease(job).await?;
                        self.app
                            .update_referee_round(
                                &self.referee_account,
                                &submission.id,
                                number,
                                RoundUpdate::Referee(step),
                            )
                            .await?;
                        return self.defer_referee(job).await;
                    }
                };
                match status {
                    OracleStatus::Queued { position } => {
                        if *queue_position != position {
                            step.state = StepState::Running {
                                task: Some(task.clone()),
                                queue_position: position,
                                since: *since,
                            };
                            self.ensure_lease(job).await?;
                            self.app
                                .update_referee_round(
                                    &self.referee_account,
                                    &submission.id,
                                    number,
                                    RoundUpdate::Referee(step),
                                )
                                .await?;
                        }
                        return self.defer_referee(job).await;
                    }
                    OracleStatus::Running => {
                        if queue_position.is_some() {
                            step.state = StepState::Running {
                                task: Some(task.clone()),
                                queue_position: None,
                                since: *since,
                            };
                            self.ensure_lease(job).await?;
                            self.app
                                .update_referee_round(
                                    &self.referee_account,
                                    &submission.id,
                                    number,
                                    RoundUpdate::Referee(step),
                                )
                                .await?;
                        }
                        return self.defer_referee(job).await;
                    }
                    OracleStatus::Completed { text, model } => {
                        if let Some(model) = model {
                            step.model = Some(model);
                        }
                        let filed_model = step
                            .model
                            .clone()
                            .unwrap_or_else(|| self.referee_model.clone());
                        let report = match parse_answer::<RefereeOut>(&text) {
                            Ok(raw) => mapping::referee_for_kind(raw, text, &submission.claims, submission.kind == wishpool_core::model::SubmissionKind::Conjecture),
                            Err(error) => mapping::referee(RefereeOut { summary: "The referee answer could not be parsed as a structured report.".into(), limits: vec![error.to_string()], ..Default::default() }, text, &submission.claims),
                        };
                        step.state = StepState::Done {
                            result: report.clone(),
                            at: chrono::Utc::now(),
                        };
                        self.ensure_lease(job).await?;
                        self.app
                            .update_referee_round(
                                &self.referee_account,
                                &submission.id,
                                number,
                                RoundUpdate::Referee(step),
                            )
                            .await?;
                        for reading in &report.claims {
                            if let Some(output) = mapping::referee_judgement(reading) {
                                self.ensure_lease(job).await?;
                                match self
                                    .app
                                    .file_machine_judgement(
                                        &self.referee_account,
                                        &submission.id,
                                        &reading.claim,
                                        output,
                                        FiledBy {
                                            engine: "nyxid-oracle".into(),
                                            model: Some(filed_model.clone()),
                                        },
                                    )
                                    .await
                                {
                                    Ok(_) => {}
                                    Err(CoreError::Conflict(reason)) => {
                                        tracing::info!(claim = %reading.claim, reason, "referee judgement conflict")
                                    }
                                    Err(error) => return Err(error.into()),
                                }
                            }
                        }
                        return self.defer_referee(job).await;
                    }
                    OracleStatus::Failed { reason, detail } => {
                        let can_retry = retryable(&reason);
                        step.state = self.failed(reason, detail, can_retry);
                    }
                    OracleStatus::Cancelled => {
                        step.state = self.failed("cancelled".into(), None, false);
                    }
                }
                self.ensure_lease(job).await?;
                self.app
                    .update_referee_round(
                        &self.referee_account,
                        &submission.id,
                        number,
                        RoundUpdate::Referee(step),
                    )
                    .await?;
                return self.defer_referee(job).await;
            }
            _ => {}
        }
        if !round.audit.is_settled() {
            let mut audit = round.audit.clone();
            if let (Some(report), Some(advisor)) = (round.referee.done(), &self.advisor) {
                match self.advisor_input(submission, report).await {
                    Ok((input, _work, source)) => {
                        audit.engine = Some(advisor.engine().into());
                        audit.model = Some(advisor.model().into());
                        audit.input_digest =
                            Some(input_digest(&referee_prompts::audit(&input), &source));
                        audit.attempts += 1;
                        audit.state = StepState::Running {
                            task: None,
                            queue_position: None,
                            since: chrono::Utc::now(),
                        };
                        self.ensure_lease(job).await?;
                        self.app
                            .update_referee_round(
                                &self.auditor_account,
                                &submission.id,
                                number,
                                RoundUpdate::Audit(audit.clone()),
                            )
                            .await?;
                        audit.state =
                            match tokio::time::timeout(self.audit_budget, advisor.audit(&input))
                                .await
                            {
                                Ok(Ok(raw)) => {
                                    match mapping::audit(raw, &input, &submission.claims) {
                                        Ok(result) => StepState::Done {
                                            result,
                                            at: chrono::Utc::now(),
                                        },
                                        Err(error) => self.failed(error.to_string(), None, false),
                                    }
                                }
                                Ok(Err(error)) if transient(&error) => {
                                    return Err(Failure::Transient(error.to_string()));
                                }
                                Ok(Err(error)) => self.failed(error.to_string(), None, false),
                                Err(_) => {
                                    return Err(Failure::Transient(
                                        "the audit exceeded its time budget".into(),
                                    ));
                                }
                            };
                    }
                    Err(error) => audit.state = self.failed(error, None, false),
                }
            } else {
                audit.state = if round.referee.done().is_some() {
                    self.failed("no Codex auditor configured".into(), None, false)
                } else {
                    StepState::Skipped {
                        reason: "no completed referee report".into(),
                    }
                };
            }
            self.ensure_lease(job).await?;
            self.app
                .update_referee_round(
                    &self.auditor_account,
                    &submission.id,
                    number,
                    RoundUpdate::Audit(audit),
                )
                .await?;
            return self.defer_referee(job).await;
        }
        // Replaying this persisted audit is safe after either report or record writes.
        let decided = if round.audit.done().is_some() {
            self.ensure_lease(job).await?;
            self.app
                .apply_referee_audit(&self.auditor_account, &submission.id, number)
                .await?
        } else {
            submission.clone()
        };
        let submission = &decided;
        if !round.advice.is_settled() {
            let mut advice = round.advice.clone();
            if round.audit.done().is_none() {
                advice.state = StepState::Skipped {
                    reason: "no completed audit".into(),
                };
            } else if let Some(advisor) = &self.advisor {
                let prepared = self
                    .advisor_input(
                        submission,
                        round.referee.done().expect("audit follows report"),
                    )
                    .await;
                match prepared {
                    Ok((mut input, _work, source)) => {
                        input.audit = round
                            .audit
                            .done()
                            .and_then(|a| serde_json::to_value(a).ok());
                        input.decision = serde_json::to_value(&submission.status).ok();
                        advice.engine = Some(advisor.engine().into());
                        advice.model = Some(advisor.model().into());
                        advice.input_digest =
                            Some(input_digest(&referee_prompts::advice(&input), &source));
                        advice.attempts += 1;
                        advice.state = StepState::Running {
                            task: None,
                            queue_position: None,
                            since: chrono::Utc::now(),
                        };
                        self.ensure_lease(job).await?;
                        self.app
                            .update_referee_round(
                                &self.referee_account,
                                &submission.id,
                                number,
                                RoundUpdate::Advice(advice.clone()),
                            )
                            .await?;
                        advice.state = match advisor.advise(&input).await {
                            Ok(raw) => StepState::Done {
                                result: mapping::advice(raw, &submission.claims),
                                at: chrono::Utc::now(),
                            },
                            Err(error) => self.failed(error.to_string(), None, transient(&error)),
                        };
                    }
                    Err(error) => {
                        advice.state = self.failed(error, None, false);
                    }
                }
            } else {
                advice.state = self.failed("no advisor configured".into(), None, false);
            }
            self.ensure_lease(job).await?;
            self.app
                .update_referee_round(
                    &self.referee_account,
                    &submission.id,
                    number,
                    RoundUpdate::Advice(advice),
                )
                .await?;
            // Release between calls so other papers can progress.
            return self.defer_referee(job).await;
        }
        if !round.letter.is_settled() {
            let mut letter = round.letter.clone();
            if let (Some(report), Some(audit)) = (round.referee.done(), round.audit.done()) {
                if let Some(advisor) = &self.advisor {
                    match self.advisor_input(submission, report).await {
                        Ok((mut input, _work, source)) => {
                            input.audit = serde_json::to_value(audit).ok();
                            input.decision = Some(
                                serde_json::json!({ "status": submission.status, "decision": submission.decision }),
                            );
                            let advice = round.advice.done().map(mapping::advice_out);
                            letter.engine = Some(advisor.engine().into());
                            letter.model = Some(advisor.model().into());
                            letter.input_digest = Some(input_digest(
                                &referee_prompts::letter(&input, advice.as_ref()),
                                &source,
                            ));
                            letter.attempts += 1;
                            letter.state = StepState::Running {
                                task: None,
                                queue_position: None,
                                since: chrono::Utc::now(),
                            };
                            self.ensure_lease(job).await?;
                            self.app
                                .update_referee_round(
                                    &self.referee_account,
                                    &submission.id,
                                    number,
                                    RoundUpdate::Letter(letter.clone()),
                                )
                                .await?;
                            letter.state = match advisor
                                .draft_letter(&input, advice.as_ref())
                                .await
                                .and_then(mapping::letter)
                            {
                                Ok(result) => StepState::Done {
                                    result,
                                    at: chrono::Utc::now(),
                                },
                                Err(error) => {
                                    self.failed(error.to_string(), None, transient(&error))
                                }
                            };
                        }
                        Err(error) => {
                            letter.state = self.failed(error, None, false);
                        }
                    }
                } else {
                    letter.state = self.failed("no advisor configured".into(), None, false);
                }
            } else {
                letter.state = StepState::Skipped {
                    reason: "no completed audit".into(),
                };
            }
            self.ensure_lease(job).await?;
            self.app
                .update_referee_round(
                    &self.auditor_account,
                    &submission.id,
                    number,
                    RoundUpdate::Letter(letter),
                )
                .await?;
            return self.defer_referee(job).await;
        }
        if !round.formal.is_settled() {
            if matches!(submission.status, SubmissionStatus::Accepted { .. })
                && round.letter.done().is_some()
            {
                self.app
                    .queue_lean_statements(&self.referee_account, &submission.id)
                    .await?;
            }
            if submission.kind == wishpool_core::model::SubmissionKind::Conjecture {
                if matches!(submission.status, SubmissionStatus::Accepted { .. })
                    && round.letter.done().is_some()
                {
                    self.app
                        .queue_lean_statements(&self.referee_account, &submission.id)
                        .await?;
                }
                let mut formal = round.formal.clone();
                formal.state = StepState::Skipped {
                    reason: "Conjecture statements use author confirmation, not a proof probe."
                        .into(),
                };
                self.app
                    .update_referee_round(
                        &self.referee_account,
                        &submission.id,
                        number,
                        RoundUpdate::Formal(formal),
                    )
                    .await?;
                return self.defer_referee(job).await;
            }
            let mut formal = round.formal.clone();
            let targets = round
                .advice
                .done()
                .map(|advice| mapping::formal_targets(advice, &submission.claims))
                .unwrap_or_default();
            match (&self.formalizer, round.advice.done()) {
                _ if !matches!(submission.status, SubmissionStatus::Accepted { .. }) => {
                    formal.state = StepState::Skipped {
                        reason: "the paper was not accepted".into(),
                    };
                }
                _ if round.letter.done().is_none() => {
                    formal.state = StepState::Skipped {
                        reason: "no delivered letter".into(),
                    };
                }
                (None, _) => {
                    formal.state = StepState::Skipped {
                        reason: "no Lean workspace is configured".into(),
                    };
                }
                (_, None) => {
                    formal.state = StepState::Skipped {
                        reason: "there is no advice to take candidates from".into(),
                    };
                }
                _ if targets.is_empty() => {
                    formal.state = StepState::Skipped {
                        reason: "the advice proposes no tractable statement".into(),
                    };
                }
                (Some(formalizer), Some(_)) => {
                    let report = round.referee.done().expect("advice follows a report");
                    match self.advisor_input(submission, report).await {
                        Ok((advisor_input, work, source)) => {
                            let input = wishpool_review::lean::FormalInput {
                                conjecture: false,
                                correction: None,
                                title: advisor_input.title,
                                abstract_text: advisor_input.abstract_text,
                                statements: advisor_input.statements,
                                targets,
                                source_dir: advisor_input.source_dir,
                                main_file: advisor_input.main_file,
                            };
                            formal.engine = Some(formalizer.engine().into());
                            formal.model = Some(formalizer.model().into());
                            formal.input_digest = Some(input_digest(
                                &serde_json::to_string(&input).unwrap_or_default(),
                                &source,
                            ));
                            formal.attempts += 1;
                            formal.state = StepState::Running {
                                task: None,
                                queue_position: None,
                                since: chrono::Utc::now(),
                            };
                            self.ensure_lease(job).await?;
                            self.app
                                .update_referee_round(
                                    &self.referee_account,
                                    &submission.id,
                                    number,
                                    RoundUpdate::Formal(formal.clone()),
                                )
                                .await?;
                            let probe_dir = work.path().join("probe");
                            formal.state = match tokio::time::timeout(
                                self.formal_budget,
                                formalizer.formalize(&input, &probe_dir),
                            )
                            .await
                            {
                                Ok(Ok(out)) => StepState::Done {
                                    result: mapping::formal(out, &input.targets),
                                    at: chrono::Utc::now(),
                                },
                                Ok(Err(error)) => {
                                    self.failed(error.to_string(), None, transient(&error))
                                }
                                Err(_) => self.failed(
                                    "the formalization step exceeded its time budget".into(),
                                    None,
                                    false,
                                ),
                            };
                        }
                        Err(error) => formal.state = self.failed(error, None, false),
                    }
                }
            }
            self.ensure_lease(job).await?;
            self.app
                .update_referee_round(
                    &self.referee_account,
                    &submission.id,
                    number,
                    RoundUpdate::Formal(formal),
                )
                .await?;
            return self.defer_referee(job).await;
        }
        Ok(false)
    }

    pub(super) async fn lean_statement_job(&self, job: &LeasedJob) -> Result<bool, Failure> {
        use wishpool_core::model::{LeanStatementResponse, SubmissionKind};
        let submission = self
            .app
            .submission(&self.referee_account, &job.submission)
            .await?;
        if !matches!(submission.status, SubmissionStatus::Accepted { .. }) {
            return Ok(false);
        }
        let Some(formalizer) = &self.formalizer else {
            return Ok(false);
        };
        if let Some(old) = submission.lean_statements.iter().find(|a| {
            a.version == submission.current_version().unwrap().number
                && a.claims_revision == submission.claims_revision
                && !a.lean.contains("def wishpool_target_prop")
                && !matches!(a.response, LeanStatementResponse::Rejected { .. })
        }) {
            let work = tempfile::tempdir_in(&self.advisor_work_dir)
                .map_err(|e| Failure::Transient(e.to_string()))?;
            let converted = match formalizer
                .elaborate_target(old.claim.as_str(), &old.lean, work.path())
                .await
            {
                Ok(out) => out
                    .files
                    .into_iter()
                    .find(|f| f.compiled)
                    .map(|f| (f.lean, out.toolchain)),
                Err(_) => None,
            };
            self.ensure_lease(job).await?;
            self.app
                .replace_legacy_target(
                    &self.referee_account,
                    &submission.id,
                    &old.digest,
                    converted,
                )
                .await?;
            self.jobs.defer(job, std::time::Duration::ZERO).await?;
            return Ok(true);
        }
        let file = self
            .app
            .referee(&self.referee_account, &submission.id)
            .await?;
        let primary_report = file
            .rounds
            .last()
            .filter(|r| {
                r.version == submission.current_version().unwrap().number
                    && r.claims_revision == submission.claims_revision
                    && r.letter.as_ref().and_then(|s| s.done()).is_some()
            })
            .and_then(|r| r.referee.done());
        if submission.kind == SubmissionKind::Conjecture && primary_report.is_none() {
            return Ok(false);
        }
        let version = submission.current_version().unwrap().number;
        let eligible = self
            .app
            .open_problem_files(&self.referee_account, &submission.id)
            .await?;
        for claim in submission
            .claims
            .iter()
            .filter(|c| c.kind.is_open() && eligible.iter().any(|f| f.claim == c.id && f.admitted))
        {
            let latest = submission
                .lean_statements
                .iter()
                .rev()
                .find(|a| a.claim == claim.id);
            let correction = match latest.map(|a| &a.response) {
                Some(LeanStatementResponse::Rejected { comment, .. }) => Some(comment.clone()),
                Some(_) => continue,
                None => None,
            };
            // A paper problem has its own audited report. Older accepted papers
            // need no primary referee letter to prepare that independent target.
            let report = if submission.kind == SubmissionKind::Conjecture {
                primary_report
            } else {
                eligible
                    .iter()
                    .find(|f| f.claim == claim.id && f.admitted)
                    .and_then(|f| f.report.as_ref())
            };
            let Some(report) = report else {
                continue;
            };
            let (prepared, work, _) = self
                .advisor_input(&submission, report)
                .await
                .map_err(Failure::Permanent)?;
            let input = wishpool_review::lean::FormalInput {
                conjecture: true,
                correction,
                title: prepared.title,
                abstract_text: prepared.abstract_text,
                statements: prepared.statements,
                targets: vec![wishpool_review::lean::FormalTarget {
                    claim: claim.id.to_string(),
                    label: claim.label.clone(),
                    statement: claim.statement.clone(),
                    lean_sketch: latest.map(|a| a.lean.clone()).unwrap_or_default(),
                    plan: "Translate the exact conjecture and explain it to the author.".into(),
                    mathlib: vec![],
                }],
                source_dir: prepared.source_dir,
                main_file: prepared.main_file,
            };
            let out = tokio::time::timeout(
                self.formal_budget,
                formalizer.formalize(&input, &work.path().join("target")),
            )
            .await
            .map_err(|_| Failure::Transient("Lean statement elaboration timed out".into()))?
            .map_err(|e| Failure::Transient(e.to_string()))?;
            let checked = out
                .files
                .iter()
                .find(|f| {
                    f.claim == claim.id.as_str()
                        && f.compiled
                        && f.theorem.as_deref() == Some("wishpool_target_prop")
                })
                .ok_or_else(|| {
                    Failure::Transient("no faithful elaborated Lean statement was returned".into())
                })?;
            self.ensure_lease(job).await?;
            self.app
                .record_lean_statement(
                    &self.referee_account,
                    &submission.id,
                    version,
                    submission.claims_revision,
                    &claim.id,
                    checked.lean.clone(),
                    out.toolchain,
                    checked.note.clone(),
                    latest.map(|a| a.digest.clone()),
                )
                .await?;
        }
        Ok(false)
    }

    fn failed<T>(&self, reason: String, detail: Option<String>, retryable: bool) -> StepState<T> {
        StepState::Failed {
            reason,
            detail,
            retryable,
            at: chrono::Utc::now(),
        }
    }

    async fn defer_referee(&self, job: &LeasedJob) -> Result<bool, Failure> {
        self.jobs.defer(job, self.oracle_poll).await?;
        Ok(true)
    }

    pub(super) async fn advisor_input(
        &self,
        paper: &Submission,
        report: &wishpool_core::model::RefereeReport,
    ) -> Result<(AdvisorInput, tempfile::TempDir, Vec<u8>), String> {
        let version = paper.current_version().ok_or("the paper has no version")?;
        let file = self
            .app
            .paper_file(
                Some(&self.referee_account),
                &paper.id,
                Some(version.number),
                false,
            )
            .await
            .map_err(|e| e.to_string())?;
        let files = wishpool_latex::archive::unpack(
            &file.bytes,
            &version.filename,
            wishpool_latex::archive::Limits::default(),
        )
        .map_err(|e| e.to_string())?;
        let main = wishpool_latex::parse::main_file(&files).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&self.advisor_work_dir).map_err(|e| e.to_string())?;
        let work = tempfile::tempdir_in(&self.advisor_work_dir).map_err(|e| e.to_string())?;
        let source_dir = work.path().join("source");
        std::fs::create_dir(&source_dir).map_err(|e| e.to_string())?;
        // Canonical archive content includes every file and its path, including binary data.
        let mut source = Vec::new();
        for (name, bytes) in files {
            source.extend_from_slice(&(name.len() as u64).to_be_bytes());
            source.extend_from_slice(name.as_bytes());
            source.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
            source.extend_from_slice(&bytes);
            let path = source_dir.join(&name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(path, bytes).map_err(|e| e.to_string())?;
        }
        let input = AdvisorInput {
            kind: serde_json::to_value(paper.kind)
                .unwrap()
                .as_str()
                .unwrap()
                .into(),
            title: paper.title.clone(),
            abstract_text: paper.abstract_text.clone(),
            authors: paper.authors.iter().map(|a| a.name.clone()).collect(),
            statements: mapping::confirmed_statements(&paper.claims),
            referee: mapping::referee_out(report),
            audit: None,
            decision: None,
            text: crate::latex::source_text(&file.bytes, &version.filename).ok(),
            source_dir: Some(source_dir),
            main_file: Some(main),
        };
        Ok((input, work, source))
    }
}

#[cfg(test)]
mod tests;
