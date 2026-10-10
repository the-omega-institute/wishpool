//! Volunteer legwork on papers whose authors opted in: tasks, leases,
//! contributions, verification and credit.
//!
//! Work no machine can check, an escape judgement, counts once independent
//! judgements from different model families and accounts agree, or an
//! editor decides (a quorum, as BOINC validates results). A literature check
//! or a probe counts once an editor accepts it. A formalization counts once
//! an editor records the checked Lean proof it produced.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{App, clamp};
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, PersonId, SubmissionId, new_uuid},
    judgement::{ClaimJudgement, Standing, combine},
    model::*,
    ports::{ContributionFilter, Listing, TaskFilter},
};

/// Concurrent leases one contributor may hold.
pub const MAX_ACTIVE_LEASES: u32 = 5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGeneration {
    pub created: u32,
    pub existing: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconciliation {
    pub leases_expired: u32,
}

/// What a contributor holding a task may read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskContext {
    pub task: Task,
    pub paper_title: String,
    pub abstract_text: String,
    pub claim: Claim,
    /// The statements this one depends on, as the author confirmed them.
    pub dependencies: Vec<Claim>,
    /// Where formalizations go, for formalization tasks.
    pub repository: Option<String>,
    /// The PDF is public (the paper is accepted).
    pub pdf_public: bool,
    /// Math macros of the current version, for rendering the statement.
    pub macros: std::collections::BTreeMap<String, String>,
    pub nature: &'static str,
}

impl App {
    pub async fn task(&self, id: &str) -> CoreResult<Task> {
        self.ports
            .tasks
            .get(id)
            .await?
            .ok_or_else(|| CoreError::not_found("task", id))
    }

    async fn replace_task(&self, task: &mut Task) -> CoreResult<()> {
        let expected = task.revision;
        task.updated_at = self.ports.clock.now();
        task.revision = expected + 1;
        self.ports.tasks.replace(task, expected).await
    }

    async fn replace_contribution(&self, contribution: &mut Contribution) -> CoreResult<()> {
        let expected = contribution.revision;
        contribution.updated_at = self.ports.clock.now();
        contribution.revision = expected + 1;
        self.ports
            .contributions
            .replace(contribution, expected)
            .await
    }

    async fn insert_task(
        &self,
        created_by: &PersonId,
        kind: TaskKind,
        submission: &Submission,
        claim: &Claim,
    ) -> CoreResult<bool> {
        let now = self.ports.clock.now();
        let verb = match kind {
            TaskKind::JudgeEscape => "Judge",
            TaskKind::LiteratureCheck => "Search the literature for",
            TaskKind::Formalize => "Formalize",
            TaskKind::Probe => "Work on",
        };
        let target = TaskTarget {
            submission: submission.id.clone(),
            claim: claim.id.clone(),
        };
        let task = Task {
            id: new_uuid(),
            kind,
            dedupe_key: Task::dedupe(kind, &target, submission.claims_revision),
            target,
            title: format!("{verb} {}", claim.label),
            contributors: vec![],
            status: TaskStatus::Open,
            created_by: created_by.clone(),
            created_at: now,
            updated_at: now,
            revision: 0,
        };
        self.ports.tasks.insert_if_absent(&task).await
    }

    /// Open one task on a statement (after acceptance: formalize, probe).
    pub(crate) async fn open_task(
        &self,
        caller: &Caller,
        id: &SubmissionId,
        claim: &ClaimId,
        kind: TaskKind,
    ) -> CoreResult<bool> {
        let submission = self.load(id).await?;
        let claim = submission
            .claim(claim)
            .ok_or_else(|| CoreError::not_found("statement", claim.as_str()))?
            .clone();
        self.insert_task(&caller.person, kind, &submission, &claim)
            .await
    }

    /// Review tasks for a paper in review whose author opted in: a judgement
    /// for every proved statement, a literature check for every main result.
    pub(crate) async fn cut_review_tasks(
        &self,
        created_by: &PersonId,
        submission: &Submission,
    ) -> CoreResult<TaskGeneration> {
        let mut outcome = TaskGeneration::default();
        if !(submission.is_open() && submission.open_to_contributors) {
            return Ok(outcome);
        }
        for claim in submission.claims.iter().filter(|c| !c.kind.is_open()) {
            let mut kinds = vec![TaskKind::JudgeEscape];
            if claim.is_main_result() {
                kinds.push(TaskKind::LiteratureCheck);
            }
            for kind in kinds {
                if self
                    .insert_task(created_by, kind, submission, claim)
                    .await?
                {
                    outcome.created += 1;
                } else {
                    outcome.existing += 1;
                }
            }
        }
        Ok(outcome)
    }

    pub async fn generate_review_tasks(
        &self,
        caller: &Caller,
        id: &SubmissionId,
    ) -> CoreResult<TaskGeneration> {
        caller.require(Role::Editor)?;
        let submission = self.submission(caller, id).await?;
        if !submission.open_to_contributors {
            return Err(CoreError::conflict(
                "the author has not opened this paper to contributors",
            ));
        }
        if !submission.is_open() {
            return Err(CoreError::conflict(format!(
                "the paper is {}",
                submission.status.name()
            )));
        }
        self.cut_review_tasks(&caller.person, &submission).await
    }

    /// Close every unfinished task of a paper (the author closed it to
    /// contributors, or withdrew it).
    pub(crate) async fn close_tasks(&self, id: &SubmissionId, reason: &str) -> CoreResult<()> {
        let filter = TaskFilter {
            submission: Some(id.clone()),
            ..TaskFilter::default()
        };
        for mut task in self.ports.tasks.list(&filter, 500, None).await?.items {
            if matches!(task.status, TaskStatus::Open | TaskStatus::Leased { .. }) {
                task.status = TaskStatus::Closed {
                    reason: reason.to_owned(),
                };
                self.replace_task(&mut task).await?;
            }
        }
        Ok(())
    }

    pub async fn list_tasks(
        &self,
        filter: TaskFilter,
        limit: Option<u32>,
        before: Option<String>,
    ) -> CoreResult<Listing<Task>> {
        self.ports.tasks.list(&filter, clamp(limit), before).await
    }

    /// The material a task needs. Staff always; contributors while the
    /// author keeps the paper open to them.
    pub async fn task_context(&self, caller: &Caller, id: &str) -> CoreResult<TaskContext> {
        let task = self.task(id).await?;
        let submission = self.load(&task.target.submission).await?;
        if !(Self::is_staff(caller) || submission.open_to_contributors) {
            return Err(CoreError::not_found("task", id));
        }
        let claim = submission
            .claim(&task.target.claim)
            .ok_or_else(|| CoreError::not_found("statement", task.target.claim.as_str()))?
            .clone();
        let dependencies = claim
            .depends_on
            .iter()
            .filter_map(|d| submission.claim(d).cloned())
            .collect();
        Ok(TaskContext {
            nature: task.kind.nature(),
            paper_title: submission.title.clone(),
            abstract_text: submission.abstract_text.clone(),
            claim,
            dependencies,
            repository: submission.formalization.repository.clone(),
            pdf_public: matches!(submission.status, SubmissionStatus::Accepted { .. }),
            macros: submission
                .current_version()
                .map(|v| v.macros.clone())
                .unwrap_or_default(),
            task,
        })
    }

    pub async fn lease_task(
        &self,
        caller: &Caller,
        id: &str,
        mode: ContributionMode,
    ) -> CoreResult<Task> {
        let now = self.ports.clock.now();
        let mut task = self.task(id).await?;
        let submission = self.load(&task.target.submission).await?;
        if !submission.open_to_contributors {
            return Err(CoreError::conflict(
                "the author has closed this paper to contributors",
            ));
        }
        if submission.involves(&caller.person) {
            return Err(CoreError::forbidden(
                "authors cannot take tasks on their own paper",
            ));
        }
        let accepted = matches!(submission.status, SubmissionStatus::Accepted { .. });
        let ready = match task.kind {
            TaskKind::JudgeEscape | TaskKind::LiteratureCheck => submission.is_open(),
            TaskKind::Formalize | TaskKind::Probe => accepted,
        };
        if !ready {
            return Err(CoreError::conflict(format!(
                "the paper is {}",
                submission.status.name()
            )));
        }
        if mode == ContributionMode::Hosted && !task.kind.hosted_capable() {
            return Err(CoreError::invalid("this task needs a full agent"));
        }
        if task.kind == TaskKind::Formalize && submission.formalization.repository.is_none() {
            return Err(CoreError::conflict(
                "the formalization repository is not set yet",
            ));
        }
        if !task.is_leasable(now) {
            return Err(CoreError::conflict(format!(
                "the task is {}",
                task.status.name()
            )));
        }
        if task.contributors.contains(&caller.person) {
            return Err(CoreError::conflict(
                "you already contributed to this task; independent contributors are needed",
            ));
        }
        if self.ports.tasks.active_leases(&caller.person, now).await? >= MAX_ACTIVE_LEASES {
            return Err(CoreError::conflict(format!(
                "at most {MAX_ACTIVE_LEASES} active leases"
            )));
        }
        task.status = TaskStatus::Leased {
            lease: Lease {
                holder: caller.person.clone(),
                mode,
                until: now + task.kind.lease_duration(),
            },
        };
        self.replace_task(&mut task).await?;
        Ok(task)
    }

    pub async fn release_task(&self, caller: &Caller, id: &str) -> CoreResult<Task> {
        let mut task = self.task(id).await?;
        match &task.status {
            TaskStatus::Leased { lease } if lease.holder == caller.person => {}
            _ => return Err(CoreError::conflict("you do not hold a lease on this task")),
        }
        task.status = TaskStatus::Open;
        self.replace_task(&mut task).await?;
        Ok(task)
    }

    /// Hand in a result for a leased task. Token usage reported through this
    /// route is recorded as unmetered.
    pub async fn submit_contribution(
        &self,
        caller: &Caller,
        task_id: &str,
        mut new: NewContribution,
    ) -> CoreResult<Contribution> {
        if let Some(tokens) = &mut new.tokens {
            tokens.metered = false;
        }
        self.submit_contribution_as(caller, task_id, new).await
    }

    /// Composition-only: the hosted worker submits with usage metered by
    /// NyxID.
    pub async fn submit_contribution_as(
        &self,
        caller: &Caller,
        task_id: &str,
        new: NewContribution,
    ) -> CoreResult<Contribution> {
        new.agent.validate()?;
        let now = self.ports.clock.now();
        let mut task = self.task(task_id).await?;
        let mode = match &task.status {
            TaskStatus::Leased { lease } if lease.holder == caller.person && lease.until > now => {
                lease.mode
            }
            _ => {
                return Err(CoreError::conflict(
                    "submit requires your unexpired lease on the task",
                ));
            }
        };
        new.output.validate(task.kind)?;
        if let ContributionOutput::PullRequest { url } = &new.output {
            let submission = self.load(&task.target.submission).await?;
            let repository = submission.formalization.repository.unwrap_or_default();
            if repository.is_empty()
                || !url.starts_with(&format!("{}/pull/", repository.trim_end_matches('/')))
            {
                return Err(CoreError::invalid(
                    "the pull request must be to the paper's formalization repository",
                ));
            }
        }
        let contribution = Contribution {
            id: new_uuid(),
            task: task.id.clone(),
            kind: task.kind,
            contributor: caller.person.clone(),
            mode,
            agent: new.agent.clone(),
            output: new.output.clone(),
            tokens: new.tokens,
            status: ContributionStatus::Submitted,
            submitted_at: now,
            updated_at: now,
            revision: 0,
        };
        self.ports.contributions.insert(&contribution).await?;
        task.contributors.push(caller.person.clone());
        // Judgement tasks stay open for independent judgements until settled.
        task.status = match task.kind {
            TaskKind::JudgeEscape => TaskStatus::Open,
            _ => TaskStatus::Submitted {
                contribution: contribution.id.clone(),
            },
        };
        self.replace_task(&mut task).await?;

        if let ContributionOutput::Judgement {
            shape,
            witnesses,
            rationale,
        } = &new.output
        {
            let submission = self.load(&task.target.submission).await?;
            if !submission.is_open() {
                return Err(CoreError::conflict(format!(
                    "the paper is {}",
                    submission.status.name()
                )));
            }
            let judgement = ClaimJudgement {
                id: new_uuid(),
                submission: task.target.submission.clone(),
                claim: task.target.claim.clone(),
                claims_revision: submission.claims_revision,
                shape: *shape,
                witnesses: witnesses.clone(),
                rationale: rationale.clone(),
                reviewer: ReviewerIdentity::Machine {
                    account: caller.person.clone(),
                    engine: new.agent.tool.clone(),
                    model: Some(new.agent.model.clone()),
                },
                task: Some(task.id.clone()),
                filed_at: now,
            };
            self.ports.judgements.insert(&judgement).await?;
            self.settle_judge_tasks(&task.target.submission, &task.target.claim)
                .await?;
        }
        self.ports
            .contributions
            .get(&contribution.id)
            .await?
            .ok_or_else(|| CoreError::not_found("contribution", &contribution.id))
    }

    /// Re-evaluate a statement's judgements. Once confirmed or corroborated,
    /// the agreeing judgement contributions are verified, the others
    /// rejected, and the judgement task is done.
    pub(crate) async fn settle_judge_tasks(
        &self,
        id: &SubmissionId,
        claim: &ClaimId,
    ) -> CoreResult<()> {
        let revision = self.load(id).await?.claims_revision;
        let judgements = self.ports.judgements.for_submission(id).await?;
        let mine: Vec<&ClaimJudgement> = judgements
            .iter()
            .filter(|j| &j.claim == claim && j.claims_revision == revision)
            .collect();
        let Some(combined) = combine(&mine) else {
            return Ok(());
        };
        let (Some(shape), Standing::Confirmed | Standing::Corroborated) =
            (combined.shape, combined.standing)
        else {
            return Ok(());
        };
        let filter = TaskFilter {
            kind: Some(TaskKind::JudgeEscape),
            submission: Some(id.clone()),
            ..TaskFilter::default()
        };
        for mut task in self
            .ports
            .tasks
            .list(&filter, 500, None)
            .await?
            .items
            .into_iter()
            .filter(|t| &t.target.claim == claim && t.dedupe_key.ends_with(&format!("@{revision}")))
        {
            let contributions = self
                .ports
                .contributions
                .list(
                    &ContributionFilter {
                        task: Some(task.id.clone()),
                        ..Default::default()
                    },
                    100,
                    None,
                )
                .await?
                .items;
            let mut last = None;
            for mut contribution in contributions {
                if contribution.status != ContributionStatus::Submitted {
                    continue;
                }
                let agrees = matches!(&contribution.output, ContributionOutput::Judgement { shape: s, .. } if *s == shape);
                contribution.status = if agrees {
                    ContributionStatus::Verified {
                        detail: format!(
                            "{:?} across {} judgements",
                            combined.standing, combined.judgements
                        )
                        .to_lowercase(),
                    }
                } else {
                    ContributionStatus::Rejected {
                        reason: "the settled judgement disagrees".into(),
                    }
                };
                last = Some(contribution.id.clone());
                self.replace_contribution(&mut contribution).await?;
            }
            if matches!(task.status, TaskStatus::Open | TaskStatus::Leased { .. }) {
                task.status = match last {
                    Some(contribution) => TaskStatus::Done { contribution },
                    None => TaskStatus::Closed {
                        reason: "settled by an editor's judgement".into(),
                    },
                };
                self.replace_task(&mut task).await?;
            }
        }
        Ok(())
    }

    /// An editor accepts or rejects a literature check or a probe note.
    pub async fn review_contribution(
        &self,
        caller: &Caller,
        id: &str,
        accept: bool,
        note: String,
    ) -> CoreResult<Contribution> {
        caller.require(Role::Editor)?;
        let mut contribution = self
            .ports
            .contributions
            .get(id)
            .await?
            .ok_or_else(|| CoreError::not_found("contribution", id))?;
        match contribution.kind {
            TaskKind::LiteratureCheck | TaskKind::Probe => {}
            TaskKind::JudgeEscape => {
                return Err(CoreError::invalid(
                    "judgements settle by agreement or an editor's judgement",
                ));
            }
            TaskKind::Formalize if accept => {
                return Err(CoreError::invalid(
                    "formalizations count once verified on the paper's plan",
                ));
            }
            TaskKind::Formalize => {}
        }
        if contribution.status != ContributionStatus::Submitted {
            return Err(CoreError::conflict("the contribution is already settled"));
        }
        if contribution.contributor == caller.person {
            return Err(CoreError::forbidden(
                "contributors cannot review their own work",
            ));
        }
        contribution.status = if accept {
            ContributionStatus::Verified {
                detail: format!("accepted by an editor: {note}"),
            }
        } else {
            ContributionStatus::Rejected { reason: note }
        };
        self.replace_contribution(&mut contribution).await?;
        let mut task = self.task(&contribution.task).await?;
        task.status = if accept {
            TaskStatus::Done {
                contribution: contribution.id.clone(),
            }
        } else {
            TaskStatus::Open
        };
        self.replace_task(&mut task).await?;
        Ok(contribution)
    }

    /// Credit the contribution behind a verified formalization.
    pub(crate) async fn credit_formalization(
        &self,
        id: &str,
        submission: &SubmissionId,
        claim: &ClaimId,
        artifact: &FormalArtifact,
    ) -> CoreResult<()> {
        let mut contribution = self
            .ports
            .contributions
            .get(id)
            .await?
            .ok_or_else(|| CoreError::not_found("contribution", id))?;
        let mut task = self.task(&contribution.task).await?;
        if contribution.kind != TaskKind::Formalize
            || &task.target.submission != submission
            || &task.target.claim != claim
        {
            return Err(CoreError::invalid(
                "the contribution is not a formalization of this statement",
            ));
        }
        if contribution.status != ContributionStatus::Submitted {
            return Err(CoreError::conflict("the contribution is already settled"));
        }
        contribution.status = ContributionStatus::Verified {
            detail: format!("checked at {} {}", artifact.repository, artifact.commit),
        };
        self.replace_contribution(&mut contribution).await?;
        task.status = TaskStatus::Done {
            contribution: contribution.id.clone(),
        };
        self.replace_task(&mut task).await
    }

    /// Composition-only: return expired leases to the pool.
    pub async fn reconcile(&self) -> CoreResult<Reconciliation> {
        self.reconcile_attempts().await?;
        self.refresh_dependency_counts().await?;
        let now = self.ports.clock.now();
        let mut outcome = Reconciliation::default();
        for mut task in self.ports.tasks.expired_leases(now, 500).await? {
            task.status = TaskStatus::Open;
            if self.replace_task(&mut task).await.is_ok() {
                outcome.leases_expired += 1;
            }
        }
        Ok(outcome)
    }

    pub async fn list_contributions(
        &self,
        filter: ContributionFilter,
        limit: Option<u32>,
        before: Option<String>,
    ) -> CoreResult<Listing<Contribution>> {
        self.ports
            .contributions
            .list(&filter, clamp(limit), before)
            .await
    }

    /// Every contributor's record. Credit counts verified work; token totals
    /// are shown separately, metered and self-reported apart.
    pub async fn credits(&self, contributor: Option<&PersonId>) -> CoreResult<Vec<Credit>> {
        let mut by: BTreeMap<PersonId, Credit> = BTreeMap::new();
        for contribution in self.ports.contributions.all_by(contributor).await? {
            let credit = by
                .entry(contribution.contributor.clone())
                .or_insert_with(|| Credit {
                    contributor: contribution.contributor.clone(),
                    ..Credit::default()
                });
            match &contribution.status {
                ContributionStatus::Submitted => credit.submitted += 1,
                ContributionStatus::Verified { .. } => {
                    credit.verified += 1;
                    if contribution.kind == TaskKind::Formalize {
                        credit.verified_formalizations += 1;
                    }
                }
                ContributionStatus::Rejected { .. } => credit.rejected += 1,
            }
            if let Some(tokens) = contribution.tokens {
                if tokens.metered {
                    credit.metered_tokens = credit.metered_tokens.saturating_add(tokens.total());
                } else {
                    credit.reported_tokens = credit.reported_tokens.saturating_add(tokens.total());
                }
            }
        }
        let mut credits: Vec<Credit> = by.into_values().collect();
        credits.sort_by(|a, b| {
            b.verified
                .cmp(&a.verified)
                .then_with(|| a.contributor.cmp(&b.contributor))
        });
        Ok(credits)
    }
}
