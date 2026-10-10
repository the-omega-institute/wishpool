//! Each confirmed paper problem gets an independent conjecture referee and audit.
use super::{
    LeasedJob, mapping,
    worker::{Failure, Worker},
};
use wishpool_core::model::{ClaimRole, SubmissionKind};
use wishpool_review::{
    Document,
    oracle::{OracleRequest, OracleStatus},
    referee_prompts::{self, RefereeOut, parse_answer},
};
impl Worker {
    pub(super) async fn open_problems_job(&self, job: &LeasedJob) -> Result<bool, Failure> {
        let (Some(oracle), Some(advisor)) = (&self.oracle, &self.advisor) else {
            return Ok(false);
        };
        let paper = self
            .app
            .submission(&self.referee_account, &job.submission)
            .await?;
        if paper.kind == SubmissionKind::Conjecture {
            return Ok(false);
        }
        for mut file in self
            .app
            .open_problem_files(&self.referee_account, &paper.id)
            .await?
        {
            if file.audit.is_some() || file.failure.is_some() {
                continue;
            }
            let mut claim = paper.claim(&file.claim).unwrap().clone();
            claim.role = ClaimRole::Main;
            let claims = vec![claim];
            if file.report.is_none() {
                if let Some(task) = &file.task {
                    match oracle
                        .poll(task)
                        .await
                        .map_err(|e| Failure::Transient(e.to_string()))?
                    {
                        OracleStatus::Completed { text, .. } => {
                            let raw =
                                parse_answer::<RefereeOut>(&text).unwrap_or_else(|e| RefereeOut {
                                    summary: "The problem referee answer could not be parsed."
                                        .into(),
                                    limits: vec![e.to_string()],
                                    ..Default::default()
                                });
                            file.report = Some(mapping::referee_for_kind(raw, text, &claims, true));
                        }
                        OracleStatus::Failed { reason, .. } => {
                            file.failure = Some(reason);
                            self.ensure_lease(job).await?;
                            self.app
                                .record_problem_review(&self.auditor_account, file)
                                .await?;
                            continue;
                        }
                        OracleStatus::Cancelled => {
                            file.failure = Some("problem referee cancelled".into());
                            self.ensure_lease(job).await?;
                            self.app
                                .record_problem_review(&self.auditor_account, file)
                                .await?;
                            continue;
                        }
                        _ => {
                            self.jobs.defer(job, self.oracle_poll).await?;
                            return Ok(true);
                        }
                    }
                } else {
                    let source = self
                        .app
                        .paper_file(
                            Some(&self.referee_account),
                            &paper.id,
                            Some(file.version),
                            false,
                        )
                        .await?;
                    let text = crate::latex::source_text(&source.bytes, &source.filename).ok();
                    let prompt = referee_prompts::conjecture_referee(
                        &Document {
                            title: paper.title.clone(),
                            abstract_text: paper.abstract_text.clone(),
                            text,
                        },
                        &mapping::confirmed_statements(&claims),
                    );
                    let client_ref = format!("wishpool:problem:{}:r{}", file.id, file.review_round);
                    self.ensure_lease(job).await?;
                    let submitted = oracle
                        .submit(&OracleRequest {
                            prompt,
                            pdf: None,
                            client_ref: client_ref.clone(),
                            tag: client_ref,
                        })
                        .await
                        .map_err(|e| Failure::Transient(e.to_string()))?;
                    file.task = Some(submitted.task);
                }
                self.ensure_lease(job).await?;
                self.app
                    .record_problem_review(&self.auditor_account, file)
                    .await?;
                self.jobs.defer(job, self.oracle_poll).await?;
                return Ok(true);
            }
            let (mut input, _work, _) = self
                .advisor_input(&paper, file.report.as_ref().unwrap())
                .await
                .map_err(Failure::Permanent)?;
            input.kind = "conjecture".into();
            input.statements = mapping::confirmed_statements(&claims);
            let raw = tokio::time::timeout(self.audit_budget, advisor.audit(&input))
                .await
                .map_err(|_| Failure::Transient("problem audit deadline exceeded".into()))?
                .map_err(|e| Failure::Transient(e.to_string()))?;
            match mapping::audit(raw, &input, &claims) {
                Ok(audit) => file.audit = Some(audit),
                Err(error) => file.failure = Some(format!("Invalid problem audit: {error}")),
            }
            self.ensure_lease(job).await?;
            self.app
                .record_problem_review(&self.auditor_account, file)
                .await?;
        }

        Ok(false)
    }
}
