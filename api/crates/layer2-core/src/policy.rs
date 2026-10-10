//! The publication threshold: a pure function from a paper's reports and
//! endorsements to a decision. The preview an author sees and the decision
//! the auditor or an editor applies come from this one function.
//!
//! A paper is accepted when at least one of its main results is new: the
//! audit judged it to carry an escape witness (a proposition prior results
//! do not give by binding alone) and the literature check found no work
//! that states or directly implies it; or when a main result settles a
//! named, sourced open problem that the literature had not settled.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::model::{
    Endorsement, Outcome, PriorRelation, PriorWork, RejectReason, Stage, StagePayload, Submission,
    SubmissionStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionBasis {
    /// A new main result carrying an escape witness.
    EscapeWitness,
    /// A main result settles a named, sourced open problem.
    OpenProblemSettlement,
    /// An audited well-posed open conjecture with new mathematical content.
    OpenConjecture,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    /// Waiting on a stage (or the author, for `claims`).
    Pending {
        awaiting: Stage,
        detail: String,
    },
    Accept {
        basis: AdmissionBasis,
    },
    NotAccepted {
        reasons: Vec<RejectReason>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Stages whose passing report must come from a human.
    pub human_judgement: BTreeSet<Stage>,
    /// Independent endorsements required (0: none).
    pub required_endorsements: usize,
    /// Papers one author may have in draft or review at once.
    pub max_active_per_author: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            human_judgement: [Stage::Claims].into_iter().collect(),
            required_endorsements: 0,
            max_active_per_author: 3,
        }
    }
}

impl Policy {
    pub fn decide(&self, submission: &Submission, endorsements: &[Endorsement]) -> Decision {
        let pending = |awaiting: Stage, detail: String| Decision::Pending { awaiting, detail };
        if submission.status == SubmissionStatus::Draft {
            return pending(
                Stage::Claims,
                "the author has not confirmed the statements".into(),
            );
        }
        for stage in Stage::ALL {
            let Some(report) = submission.latest_report(stage) else {
                return pending(stage, format!("no current {} report", stage.code()));
            };
            match &report.outcome {
                Outcome::Fail { reason } => {
                    return Decision::NotAccepted {
                        reasons: vec![reason.clone()],
                    };
                }
                Outcome::NeedsHuman { question } => return pending(stage, question.clone()),
                Outcome::Pass
                    if self.human_judgement.contains(&stage) && !report.reviewer.is_human() =>
                {
                    return pending(stage, format!("{} needs a human report", stage.code()));
                }
                Outcome::Pass => {}
            }
        }

        if submission.kind == crate::model::SubmissionKind::Conjecture {
            return self.decide_conjecture(submission, endorsements);
        }
        let main: Vec<_> = submission
            .claims
            .iter()
            .filter(|c| c.is_main_result())
            .collect();
        if main.is_empty() {
            return Decision::NotAccepted {
                reasons: vec![RejectReason::NoMainResult],
            };
        }
        let prior: &[PriorWork] = match submission
            .latest_report(Stage::Literature)
            .map(|r| &r.payload)
        {
            Some(StagePayload::Literature { prior, .. }) => prior,
            _ => &[],
        };
        let known_by = |claim: &crate::ids::ClaimId| {
            prior
                .iter()
                .find(|p| &p.claim == claim && p.relation != PriorRelation::Related)
        };
        let assessments = match submission.latest_report(Stage::Escape).map(|r| &r.payload) {
            Some(StagePayload::Escape { assessments }) => assessments.as_slice(),
            _ => &[],
        };
        if let Some(missing) = main
            .iter()
            .find(|c| !assessments.iter().any(|a| a.claim == c.id))
        {
            return pending(
                Stage::Escape,
                format!("main result {} has no escape assessment", missing.id),
            );
        }

        let settles = main.iter().any(|c| {
            c.settles.is_some()
                && known_by(&c.id).is_none()
                && assessments.iter().any(|a| {
                    a.claim == c.id
                        && a.correctness
                            .is_none_or(|v| v == crate::model::Correctness::Correct)
                })
        });
        let new_content = main.iter().any(|c| {
            known_by(&c.id).is_none()
                && assessments
                    .iter()
                    .any(|a| a.claim == c.id && a.has_escape_content())
        });
        let basis = if new_content {
            AdmissionBasis::EscapeWitness
        } else if settles {
            AdmissionBasis::OpenProblemSettlement
        } else {
            // Report why: known results first, then binding.
            let known: Vec<RejectReason> = main
                .iter()
                .filter_map(|c| {
                    known_by(&c.id).map(|p| RejectReason::KnownResult {
                        claim: c.id.clone(),
                        prior: p.source.clone(),
                    })
                })
                .collect();
            let all_bind_only = main.iter().all(|c| {
                assessments
                    .iter()
                    .any(|a| a.claim == c.id && !a.has_escape_content())
            });
            let mut reasons = known;
            if all_bind_only || reasons.is_empty() {
                reasons.push(RejectReason::BindOnly);
            }
            return Decision::NotAccepted { reasons };
        };

        let independent = endorsements.iter().filter(|e| e.is_independent()).count();
        if independent < self.required_endorsements {
            return Decision::Pending {
                awaiting: Stage::Escape,
                detail: format!(
                    "{independent} of {} independent endorsements",
                    self.required_endorsements
                ),
            };
        }
        Decision::Accept { basis }
    }

    fn decide_conjecture(&self, submission: &Submission, endorsements: &[Endorsement]) -> Decision {
        let main: Vec<_> = submission
            .claims
            .iter()
            .filter(|c| c.role == crate::model::ClaimRole::Main && c.kind.is_open())
            .collect();
        if main.is_empty() {
            return Decision::NotAccepted {
                reasons: vec![RejectReason::Conjecture {
                    detail: "Mark at least one conjecture or question as main.".into(),
                }],
            };
        }
        let assessments = match submission.latest_report(Stage::Escape).map(|r| &r.payload) {
            Some(StagePayload::Escape { assessments }) => assessments.as_slice(),
            _ => &[],
        };
        let passes = main.iter().any(|c| {
            assessments
                .iter()
                .any(|a| a.claim == c.id && a.conjecture.as_ref().is_some_and(|r| r.displayable()))
        });
        if passes {
            let independent = endorsements.iter().filter(|e| e.is_independent()).count();
            if independent < self.required_endorsements {
                return Decision::Pending {
                    awaiting: Stage::Escape,
                    detail: format!(
                        "{independent} of {} independent endorsements",
                        self.required_endorsements
                    ),
                };
            }
            return Decision::Accept {
                basis: AdmissionBasis::OpenConjecture,
            };
        }
        let reasons = main.iter().map(|c| {
            let reading = assessments.iter().find(|a| a.claim == c.id).and_then(|a| a.conjecture.as_ref());
            let detail = match reading {
                None => format!("Statement {} has no usable check of well-posedness, open status and new content.", c.id),
                Some(r) if !r.well_posed => format!("Statement {} needs clarification: {}", c.id, r.well_posed_reason),
                Some(r) if r.status != crate::model::ConjectureStatus::Open => format!("Statement {} is not established as open: {}", c.id, r.status_reason),
                Some(r) => format!("Statement {} would only re-bind known results: {}", c.id, r.escape_reason),
            };
            RejectReason::Conjecture { detail }
        }).collect();
        Decision::NotAccepted { reasons }
    }

    /// The next stage a machine reviewer should draft: the first stage with
    /// no current report after every earlier one passed. Claims belong to
    /// the author.
    pub fn next_machine_stage(&self, submission: &Submission) -> Option<Stage> {
        if !submission.status.is_active() {
            return None;
        }
        for stage in Stage::ALL {
            match submission.latest_report(stage).map(|r| &r.outcome) {
                None if stage == Stage::Claims => return None,
                None => return Some(stage),
                Some(Outcome::Pass) => continue,
                Some(_) => return None,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
