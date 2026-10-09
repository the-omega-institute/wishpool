//! Legwork on submitted papers, done with contributors' model tokens and
//! credited only once verified.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::{
    claim::ProofShape,
    common::{is_web_url, require_text},
    review::PriorWork,
};
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, PersonId, SubmissionId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// Judge whether a statement carries new content, naming witnesses.
    JudgeEscape,
    /// Search the literature for a statement.
    LiteratureCheck,
    /// Formalize an approved statement of an accepted paper.
    Formalize,
    /// Work on a conjecture of an accepted paper that was taken up.
    Probe,
}

impl TaskKind {
    pub fn lease_duration(self) -> Duration {
        match self {
            Self::JudgeEscape => Duration::hours(2),
            Self::LiteratureCheck => Duration::hours(4),
            Self::Probe => Duration::hours(24),
            Self::Formalize => Duration::hours(72),
        }
    }

    /// Kinds a model can do alone on donated quota.
    pub fn hosted_capable(self) -> bool {
        matches!(self, Self::JudgeEscape | Self::LiteratureCheck)
    }

    /// The AI-contribution classification used for Erdős problems.
    pub fn nature(self) -> &'static str {
        match self {
            Self::LiteratureCheck => "2a-literature-search",
            Self::Formalize => "2b-formalization",
            Self::Probe => "2d-computation",
            Self::JudgeEscape => "assessment",
        }
    }
}

/// A statement of a paper.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TaskTarget {
    pub submission: SubmissionId,
    pub claim: ClaimId,
}

impl TaskTarget {
    pub fn key(&self) -> String {
        format!("{}#{}", self.submission, self.claim)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionMode {
    OwnAgent,
    Hosted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    pub holder: PersonId,
    pub mode: ContributionMode,
    pub until: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Leased { lease: Lease },
    Submitted { contribution: String },
    Done { contribution: String },
    Closed { reason: String },
}

impl TaskStatus {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Leased { .. } => "leased",
            Self::Submitted { .. } => "submitted",
            Self::Done { .. } => "done",
            Self::Closed { .. } => "closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub kind: TaskKind,
    pub target: TaskTarget,
    /// `<kind>|<target key>@<claims revision>`: one task per kind and
    /// statement of a confirmed claim set.
    pub dedupe_key: String,
    pub title: String,
    #[serde(default)]
    pub contributors: Vec<PersonId>,
    pub status: TaskStatus,
    pub created_by: PersonId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
}

impl Task {
    pub fn dedupe(kind: TaskKind, target: &TaskTarget, claims_revision: u64) -> String {
        let kind = serde_json::to_value(kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        format!("{kind}|{}@{claims_revision}", target.key())
    }

    pub fn is_leasable(&self, now: DateTime<Utc>) -> bool {
        match &self.status {
            TaskStatus::Open => true,
            TaskStatus::Leased { lease } => lease.until <= now,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInfo {
    pub tool: String,
    pub model: String,
}

impl AgentInfo {
    pub fn validate(&self) -> CoreResult<()> {
        require_text("agent tool", &self.tool, 100)?;
        require_text("agent model", &self.model, 100)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    /// True only when the NyxID gateway reported the usage; the server sets
    /// it, clients need not send it.
    #[serde(default)]
    pub metered: bool,
}

impl TokenUsage {
    pub fn total(&self) -> u64 {
        self.input.saturating_add(self.output)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "output", rename_all = "snake_case")]
pub enum ContributionOutput {
    Judgement {
        shape: ProofShape,
        witnesses: Vec<String>,
        rationale: String,
    },
    Literature {
        prior: Vec<PriorWork>,
        searched: Vec<String>,
        summary: String,
    },
    ProbeNote {
        note: String,
    },
    /// A pull request to the paper's formalization repository.
    PullRequest {
        url: String,
    },
}

impl ContributionOutput {
    pub fn validate(&self, kind: TaskKind) -> CoreResult<()> {
        match (self, kind) {
            (
                Self::Judgement {
                    shape,
                    witnesses,
                    rationale,
                },
                TaskKind::JudgeEscape,
            ) => {
                require_text("rationale", rationale, 10_000)?;
                match (shape, witnesses.iter().any(|w| !w.trim().is_empty())) {
                    (ProofShape::Content, false) => Err(CoreError::invalid(
                        "a content judgement must name a witness",
                    )),
                    (ProofShape::BindOnly, true) => {
                        Err(CoreError::invalid("a bind-only judgement names no witness"))
                    }
                    _ => Ok(()),
                }
            }
            (
                Self::Literature {
                    prior,
                    searched,
                    summary,
                },
                TaskKind::LiteratureCheck,
            ) => {
                require_text("summary", summary, 10_000)?;
                if searched.is_empty() {
                    return Err(CoreError::invalid(
                        "a literature check lists what was searched",
                    ));
                }
                prior.iter().try_for_each(|p| p.source.validate())
            }
            (Self::ProbeNote { note }, TaskKind::Probe) => require_text("note", note, 50_000),
            (Self::PullRequest { url }, TaskKind::Formalize) => {
                if is_pull_request_url(url) {
                    Ok(())
                } else {
                    Err(CoreError::invalid(
                        "expected https://github.com/<owner>/<repo>/pull/<n>",
                    ))
                }
            }
            (_, kind) => Err(CoreError::invalid(format!(
                "this output does not fit a {kind:?} task"
            ))),
        }
    }
}

pub fn is_pull_request_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    let parts: Vec<&str> = rest.trim_end_matches('/').split('/').collect();
    parts.len() == 4
        && parts[2] == "pull"
        && !parts[3].is_empty()
        && parts[3].chars().all(|c| c.is_ascii_digit())
        && is_web_url(url)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ContributionStatus {
    Submitted,
    Verified { detail: String },
    Rejected { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contribution {
    pub id: String,
    pub task: String,
    pub kind: TaskKind,
    pub contributor: PersonId,
    pub mode: ContributionMode,
    pub agent: AgentInfo,
    pub output: ContributionOutput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenUsage>,
    pub status: ContributionStatus,
    pub submitted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewContribution {
    pub agent: AgentInfo,
    pub output: ContributionOutput,
    #[serde(default)]
    pub tokens: Option<TokenUsage>,
}

/// A contributor's record. Verified artifacts count; tokens are shown
/// separately, metered and self-reported apart.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Credit {
    pub contributor: PersonId,
    pub verified: u32,
    pub submitted: u32,
    pub rejected: u32,
    pub verified_formalizations: u32,
    pub metered_tokens: u64,
    pub reported_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_fit_kinds() {
        let content_without_witness = ContributionOutput::Judgement {
            shape: ProofShape::Content,
            witnesses: vec![],
            rationale: "r".into(),
        };
        assert!(
            content_without_witness
                .validate(TaskKind::JudgeEscape)
                .is_err()
        );
        let pr = ContributionOutput::PullRequest {
            url: "https://github.com/a/b/pull/3".into(),
        };
        assert!(pr.validate(TaskKind::Formalize).is_ok());
        assert!(pr.validate(TaskKind::Probe).is_err());
        assert!(!is_pull_request_url("https://github.com/a/b/issues/3"));
        let target = TaskTarget {
            submission: "s".into(),
            claim: "C2".into(),
        };
        assert_eq!(
            Task::dedupe(TaskKind::JudgeEscape, &target, 3),
            "judge_escape|s#C2@3"
        );
    }
}
