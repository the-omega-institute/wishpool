use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ProofShape;
use crate::ids::{ClaimId, PersonId, SubmissionId};

/// Characters allowed in a sent letter.
pub const MAX_LETTER_CHARS: usize = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recommendation {
    Accept,
    MinorRevision,
    MajorRevision,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Affects correctness or a main result.
    Major,
    Minor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefereeConcern {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<ClaimId>,
    pub severity: Severity,
    pub issue: String,
}

/// The referee's reading of one statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefereeClaim {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conjecture: Option<super::ConjectureReading>,
    pub claim: ClaimId,
    pub shape: ProofShape,
    #[serde(default)]
    pub witnesses: Vec<String>,
    /// Where the statement already appears in the literature, if the
    /// referee knows a source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known: Option<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefereeReport {
    /// Absent when the referee's answer could not be read as a report; the
    /// full answer is still in `text`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<Recommendation>,
    pub summary: String,
    #[serde(default)]
    pub strengths: Vec<String>,
    #[serde(default)]
    pub concerns: Vec<RefereeConcern>,
    #[serde(default)]
    pub claims: Vec<RefereeClaim>,
    #[serde(default)]
    pub limits: Vec<String>,
    /// The referee's complete answer.
    pub text: String,
}

/// Codex's source-based audit of the referee report, never a Lean receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Correctness {
    Correct,
    Gap,
    Error,
    NotChecked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConcernStatus {
    Confirmed,
    Refuted,
    NotCheckable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditedClaim {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conjecture: Option<super::ConjectureReading>,
    pub claim: ClaimId,
    pub correctness: Correctness,
    pub comment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<ProofShape>,
    #[serde(default)]
    pub witnesses: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known: Option<String>,
    pub referee_agreed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditedConcern {
    pub concern: String,
    pub status: ConcernStatus,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefereeAudit {
    pub verdict: Recommendation,
    pub agrees_with_referee: bool,
    pub summary: String,
    pub claims: Vec<AuditedClaim>,
    pub concerns: Vec<AuditedConcern>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImprovementKind {
    /// A step of a proof that is missing or unclear.
    Gap,
    /// A stronger form of a result.
    Strengthen,
    Generalize,
    /// Computation or verification that would support a result.
    Computation,
    Literature,
    Exposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    Checked,
    #[default]
    Proposed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Improvement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<ClaimId>,
    pub kind: ImprovementKind,
    pub suggestion: String,
    /// What contributors could do for the author on this point.
    #[serde(default)]
    pub how_we_help: String,
    #[serde(default)]
    pub status: Evidence,
    #[serde(default)]
    pub evidence: String,
    pub effort: Effort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feasibility {
    /// The definitions and tools needed are in Mathlib.
    Ready,
    /// Needs definitions or lemmas that Mathlib lacks.
    NeedsLibrary,
    /// Needs substantial new formal infrastructure.
    Hard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormalizationCandidate {
    pub claim: ClaimId,
    pub feasibility: Feasibility,
    /// Mathlib notions the formal statement would use.
    #[serde(default)]
    pub mathlib: Vec<String>,
    /// Definitions or lemmas that would have to be built.
    #[serde(default)]
    pub missing: Vec<String>,
    #[serde(default)]
    pub lean_sketch: String,
    pub plan: String,
    pub effort: Effort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Advice {
    pub summary: String,
    #[serde(default)]
    pub improvements: Vec<Improvement>,
    #[serde(default)]
    pub formalization: Vec<FormalizationCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeOutcome {
    /// The Lean file compiled against Mathlib without `sorry`, and the
    /// theorem depends only on the standard axioms. Whether the formal
    /// statement says what the paper says is for a human to judge.
    Compiled,
    Failed,
}

/// One statement the formalization probe tried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormalAttempt {
    pub claim: ClaimId,
    pub outcome: ProbeOutcome,
    /// The checked declaration, when one was named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theorem: Option<String>,
    /// The complete Lean source.
    pub lean: String,
    /// Axioms the theorem depends on, as Lean reported them.
    #[serde(default)]
    pub axioms: Vec<String>,
    /// How the formal statement relates to the paper's, in the prover's words.
    #[serde(default)]
    pub note: String,
    /// Why the check failed, with the end of the compiler output.
    #[serde(default)]
    pub log: String,
}

/// A private formalization probe: never published, never a recorded
/// formalization. Publishing a Lean proof still needs the author's approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormalProbe {
    /// Lean toolchain and Mathlib revision the files were checked against.
    pub toolchain: String,
    #[serde(default)]
    pub attempts: Vec<FormalAttempt>,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetterDraft {
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum StepState<T> {
    Pending,
    Running {
        /// The external task, when the work runs elsewhere.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        queue_position: Option<u32>,
        since: DateTime<Utc>,
    },
    Done {
        result: T,
        at: DateTime<Utc>,
    },
    Failed {
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(default)]
        retryable: bool,
        at: DateTime<Utc>,
    },
    Skipped {
        reason: String,
    },
}

/// One step of a round and the engine that did (or does) it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_digest: Option<String>,
    pub state: StepState<T>,
}

impl<T> Step<T> {
    pub fn pending() -> Self {
        Self {
            engine: None,
            model: None,
            attempts: 0,
            client_ref: None,
            input_digest: None,
            state: StepState::Pending,
        }
    }

    fn not_in_round() -> Self {
        Self {
            state: StepState::Skipped {
                reason: "not part of this round".into(),
            },
            ..Self::pending()
        }
    }

    pub fn done(&self) -> Option<&T> {
        match &self.state {
            StepState::Done { result, .. } => Some(result),
            _ => None,
        }
    }

    pub fn is_settled(&self) -> bool {
        matches!(
            self.state,
            StepState::Done { .. } | StepState::Failed { .. } | StepState::Skipped { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefereeRound {
    /// 1-based, in order of starting.
    pub number: u32,
    /// The paper version and statement set the round reads.
    pub version: u32,
    pub claims_revision: u64,
    pub started_at: DateTime<Utc>,
    pub referee: Step<RefereeReport>,
    #[serde(default = "Step::not_in_round")]
    pub audit: Step<RefereeAudit>,
    pub advice: Step<Advice>,
    /// Rounds stored before the probe existed read as skipped.
    #[serde(default = "Step::not_in_round")]
    pub formal: Step<FormalProbe>,
    pub letter: Step<LetterDraft>,
}

impl RefereeRound {
    pub fn is_settled(&self) -> bool {
        self.referee.is_settled()
            && self.audit.is_settled()
            && self.advice.is_settled()
            && self.formal.is_settled()
            && self.letter.is_settled()
    }
}

/// Feedback the editors sent to the author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackLetter {
    /// The round the letter answers, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
    /// The sending editor's assessment; never a publication decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assessment: Option<Recommendation>,
    #[serde(default)]
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub edited: bool,
    pub sent_by: PersonId,
    pub sent_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStep {
    pub version: u32,
    pub claims_revision: u64,
    /// Provider-specific progress; never part of an author/public projection.
    pub progress: serde_json::Value,
}

/// Every round and letter of one paper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefereeFile {
    /// Private durable external-agent steps, including auxiliary problem/target work.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub agent_steps: std::collections::BTreeMap<String, AgentStep>,
    /// The paper's id.
    pub id: SubmissionId,
    #[serde(default)]
    pub rounds: Vec<RefereeRound>,
    #[serde(default)]
    pub letters: Vec<FeedbackLetter>,
    #[serde(default)]
    pub revision: u64,
}

impl RefereeFile {
    pub fn new(id: SubmissionId) -> Self {
        Self {
            id,
            agent_steps: Default::default(),
            rounds: vec![],
            letters: vec![],
            revision: 0,
        }
    }

    pub fn current(&self) -> Option<&RefereeRound> {
        self.rounds.last()
    }
}

/// GET projection: staff receive the full steps; authors receive no advice,
/// draft, provider metadata, or Lean files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefereeView {
    pub id: SubmissionId,
    pub rounds: Vec<RefereeRoundView>,
    pub letters: Vec<FeedbackLetter>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefereeRoundView {
    pub number: u32,
    pub version: u32,
    pub claims_revision: u64,
    pub started_at: DateTime<Utc>,
    pub referee: Step<RefereeReport>,
    pub audit: Step<RefereeAudit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advice: Option<Step<Advice>>,
    pub formal: Step<FormalProbeView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub letter: Option<Step<LetterDraft>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum FormalProbeView {
    Staff(FormalProbe),
    Author { attempts: Vec<ProbeOutcomeView> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProbeOutcomeView {
    pub claim: ClaimId,
    pub outcome: ProbeOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theorem: Option<String>,
}

impl<T: Clone> Step<T> {
    fn project<U>(&self, staff: bool, map: impl FnOnce(&T) -> U) -> Step<U> {
        let state = match &self.state {
            StepState::Pending => StepState::Pending,
            StepState::Running {
                task,
                queue_position,
                since,
            } => StepState::Running {
                task: staff.then(|| task.clone()).flatten(),
                queue_position: staff.then_some(*queue_position).flatten(),
                since: *since,
            },
            StepState::Done { result, at } => StepState::Done {
                result: map(result),
                at: *at,
            },
            StepState::Failed {
                reason,
                detail,
                retryable,
                at,
            } => StepState::Failed {
                reason: if staff {
                    reason.clone()
                } else {
                    "Review could not finish; please try again later.".into()
                },
                detail: staff.then(|| detail.clone()).flatten(),
                retryable: *retryable,
                at: *at,
            },
            StepState::Skipped { reason } => StepState::Skipped {
                reason: if staff {
                    reason.clone()
                } else {
                    "Not required.".into()
                },
            },
        };
        Step {
            engine: staff.then(|| self.engine.clone()).flatten(),
            model: staff.then(|| self.model.clone()).flatten(),
            attempts: if staff { self.attempts } else { 0 },
            client_ref: staff.then(|| self.client_ref.clone()).flatten(),
            input_digest: staff.then(|| self.input_digest.clone()).flatten(),
            state,
        }
    }
}

impl RefereeFile {
    pub fn view(self, staff: bool) -> RefereeView {
        RefereeView {
            id: self.id,
            letters: self.letters,
            revision: self.revision,
            rounds: self
                .rounds
                .into_iter()
                .map(|r| RefereeRoundView {
                    number: r.number,
                    version: r.version,
                    claims_revision: r.claims_revision,
                    started_at: r.started_at,
                    referee: r.referee.project(staff, Clone::clone),
                    audit: r.audit.project(staff, Clone::clone),
                    advice: staff.then_some(r.advice),
                    letter: staff.then_some(r.letter),
                    formal: r.formal.project(staff, |p| {
                        if staff {
                            FormalProbeView::Staff(p.clone())
                        } else {
                            FormalProbeView::Author {
                                attempts: p
                                    .attempts
                                    .iter()
                                    .map(|a| ProbeOutcomeView {
                                        claim: a.claim.clone(),
                                        outcome: a.outcome,
                                        theorem: a.theorem.clone(),
                                    })
                                    .collect(),
                            }
                        }
                    }),
                })
                .collect(),
        }
    }
}

/// A machine update to one step of a round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoundUpdate {
    Referee(Step<RefereeReport>),
    Audit(Step<RefereeAudit>),
    Advice(Step<Advice>),
    Formal(Step<FormalProbe>),
    Letter(Step<LetterDraft>),
}

/// An editor's in-app letter to the authors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewLetter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assessment: Option<Recommendation>,
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letter_assessments_round_trip_and_older_letters_remain_valid() {
        let legacy = serde_json::json!({
            "subject": "Feedback", "body": "A useful point.",
            "sent_by": "editor", "sent_at": "2026-10-08T12:00:00Z"
        });
        let letter: FeedbackLetter = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(letter.assessment, None);
        assert!(
            serde_json::to_value(&letter)
                .unwrap()
                .get("assessment")
                .is_none()
        );
        let new: NewLetter = serde_json::from_value(serde_json::json!({
            "subject": "Feedback", "body": "A useful point."
        }))
        .unwrap();
        assert_eq!(new.assessment, None);
        assert!(
            serde_json::to_value(&new)
                .unwrap()
                .get("assessment")
                .is_none()
        );

        for assessment in [
            Recommendation::Accept,
            Recommendation::MinorRevision,
            Recommendation::MajorRevision,
            Recommendation::Reject,
        ] {
            let mut letter = letter.clone();
            letter.assessment = Some(assessment);
            let value = serde_json::to_value(&letter).unwrap();
            assert_eq!(
                serde_json::from_value::<FeedbackLetter>(value).unwrap(),
                letter
            );
            let mut new = new.clone();
            new.assessment = Some(assessment);
            let value = serde_json::to_value(&new).unwrap();
            assert_eq!(serde_json::from_value::<NewLetter>(value).unwrap(), new);
        }
    }
}
