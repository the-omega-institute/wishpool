//! Escape judgements of a paper's statements from many reviewers, combined
//! by an explicit rule, and the per-paper analysis built from them.
//!
//! An editor's judgement confirms. Two machine judgements from different
//! model families and accounts that agree corroborate (a quorum, as volunteer
//! computing validates results no machine can check). Disagreement is a
//! dispute for an editor.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    ids::{ClaimId, PersonId, SubmissionId},
    model::{
        ClaimKind, ClaimRole, FormalizationPlan, ItemState, PriorRelation, ProofShape,
        ReviewerIdentity, Stage, StagePayload, Submission,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimJudgement {
    pub id: String,
    pub submission: SubmissionId,
    pub claim: ClaimId,
    /// The claim set judged; a changed set voids the judgement.
    pub claims_revision: u64,
    pub shape: ProofShape,
    #[serde(default)]
    pub witnesses: Vec<String>,
    pub rationale: String,
    pub reviewer: ReviewerIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub filed_at: DateTime<Utc>,
}

/// The provider family of a model; judgements are independent only across
/// families.
pub fn model_family(model: &str) -> String {
    let m = model.trim().to_ascii_lowercase();
    let family = if m.starts_with("claude") || m.starts_with("anthropic") {
        "anthropic"
    } else if ["gpt", "o1", "o3", "o4", "chatgpt", "codex"]
        .iter()
        .any(|p| m.starts_with(p))
    {
        "openai"
    } else if m.starts_with("gemini") || m.starts_with("gemma") {
        "google"
    } else if ["mistral", "codestral", "ministral"]
        .iter()
        .any(|p| m.starts_with(p))
    {
        "mistral"
    } else if m.starts_with("deepseek") {
        "deepseek"
    } else if m.starts_with("qwen") {
        "qwen"
    } else if m.starts_with("llama") {
        "meta"
    } else {
        return m
            .split(['-', '/', ':'])
            .next()
            .unwrap_or("unknown")
            .to_owned();
    };
    family.to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    Confirmed,
    Corroborated,
    Proposed,
    Disputed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Combined {
    pub shape: Option<ProofShape>,
    pub standing: Standing,
    pub witnesses: Vec<String>,
    pub judgements: usize,
}

pub fn combine(judgements: &[&ClaimJudgement]) -> Option<Combined> {
    if judgements.is_empty() {
        return None;
    }
    if let Some(human) = judgements
        .iter()
        .filter(|j| j.reviewer.is_human())
        .max_by_key(|j| j.filed_at)
    {
        return Some(Combined {
            shape: Some(human.shape),
            standing: Standing::Confirmed,
            witnesses: human.witnesses.clone(),
            judgements: judgements.len(),
        });
    }
    let shapes: BTreeSet<ProofShape> = judgements.iter().map(|j| j.shape).collect();
    if shapes.len() > 1 {
        return Some(Combined {
            shape: None,
            standing: Standing::Disputed,
            witnesses: vec![],
            judgements: judgements.len(),
        });
    }
    let shape = *shapes.iter().next()?;
    let mut families = BTreeSet::new();
    let mut accounts: BTreeSet<&PersonId> = BTreeSet::new();
    for judgement in judgements {
        if let ReviewerIdentity::Machine {
            account,
            model,
            engine,
        } = &judgement.reviewer
        {
            families.insert(model_family(model.as_deref().unwrap_or(engine)));
            accounts.insert(account);
        }
    }
    let standing = if families.len() >= 2 && accounts.len() >= 2 {
        Standing::Corroborated
    } else {
        Standing::Proposed
    };
    let mut witnesses: Vec<String> = judgements
        .iter()
        .flat_map(|j| j.witnesses.iter().cloned())
        .collect();
    witnesses.sort();
    witnesses.dedup();
    Some(Combined {
        shape: Some(shape),
        standing,
        witnesses,
        judgements: judgements.len(),
    })
}

/// What the analysis says about one statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimAnalysis {
    pub claim: ClaimId,
    pub kind: ClaimKind,
    pub role: ClaimRole,
    pub label: String,
    /// Prior works the editor's literature report links to this claim.
    pub prior: Vec<crate::model::PriorWork>,
    /// `same` or `implies` found: the result is already known.
    pub known: bool,
    /// The editor's escape assessment, if filed.
    pub assessment: Option<crate::model::EscapeAssessment>,
    /// Judgements gathered so far (proposals until an editor files S3).
    pub judgement: Option<Combined>,
    pub formalization: Option<ItemState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperAnalysis {
    pub submission: SubmissionId,
    pub claims: Vec<ClaimAnalysis>,
    pub main_results: usize,
    pub main_with_content: usize,
    pub main_known: usize,
    pub open_statements: usize,
    pub formalized: usize,
}

impl PaperAnalysis {
    pub fn compute(submission: &Submission, judgements: &[ClaimJudgement]) -> Self {
        let prior: Vec<crate::model::PriorWork> = match submission
            .latest_report(Stage::Literature)
            .map(|r| &r.payload)
        {
            Some(StagePayload::Literature { prior, .. }) => prior.clone(),
            _ => vec![],
        };
        let assessments = match submission.latest_report(Stage::Escape).map(|r| &r.payload) {
            Some(StagePayload::Escape { assessments }) => assessments.clone(),
            _ => vec![],
        };
        let mut by_claim: BTreeMap<&ClaimId, Vec<&ClaimJudgement>> = BTreeMap::new();
        for j in judgements.iter().filter(|j| {
            j.submission == submission.id && j.claims_revision == submission.claims_revision
        }) {
            by_claim.entry(&j.claim).or_default().push(j);
        }
        let plan: &FormalizationPlan = &submission.formalization;
        let claims: Vec<ClaimAnalysis> = submission
            .claims
            .iter()
            .map(|c| {
                let prior: Vec<_> = prior.iter().filter(|p| p.claim == c.id).cloned().collect();
                ClaimAnalysis {
                    claim: c.id.clone(),
                    kind: c.kind,
                    role: c.role,
                    label: c.label.clone(),
                    known: prior.iter().any(|p| p.relation != PriorRelation::Related),
                    prior,
                    assessment: assessments.iter().rev().find(|a| a.claim == c.id).cloned(),
                    judgement: by_claim.get(&c.id).and_then(|j| combine(j)),
                    formalization: plan.item(&c.id).map(|i| i.state.clone()),
                }
            })
            .collect();
        let main: Vec<&ClaimAnalysis> = claims
            .iter()
            .filter(|c| c.role == ClaimRole::Main && !c.kind.is_open())
            .collect();
        Self {
            submission: submission.id.clone(),
            main_results: main.len(),
            main_with_content: main
                .iter()
                .filter(|c| {
                    c.assessment
                        .as_ref()
                        .is_some_and(|a| a.has_escape_content())
                })
                .count(),
            main_known: main.iter().filter(|c| c.known).count(),
            open_statements: claims.iter().filter(|c| c.kind.is_open()).count(),
            formalized: plan.verified(),
            claims,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn judgement(
        shape: ProofShape,
        account: &str,
        model: Option<&str>,
        minutes: i64,
    ) -> ClaimJudgement {
        ClaimJudgement {
            id: format!("{account}{minutes}"),
            submission: "s".into(),
            claim: "C1".into(),
            claims_revision: 1,
            shape,
            witnesses: if shape == ProofShape::Content {
                vec!["Lemma 2".into()]
            } else {
                vec![]
            },
            rationale: "r".into(),
            reviewer: match model {
                Some(m) => ReviewerIdentity::Machine {
                    account: account.into(),
                    engine: "e".into(),
                    model: Some(m.into()),
                },
                None => ReviewerIdentity::Human {
                    person: account.into(),
                },
            },
            task: None,
            filed_at: Utc::now() + Duration::minutes(minutes),
        }
    }

    #[test]
    fn quorum_rules() {
        let a = judgement(ProofShape::Content, "u1", Some("claude-opus-5-5"), 0);
        let b = judgement(ProofShape::Content, "u2", Some("gpt-5.5"), 1);
        let same_family = judgement(ProofShape::Content, "u3", Some("claude-sonnet-5-5"), 2);
        assert_eq!(combine(&[&a, &b]).unwrap().standing, Standing::Corroborated);
        assert_eq!(
            combine(&[&a, &same_family]).unwrap().standing,
            Standing::Proposed
        );
        let against = judgement(ProofShape::BindOnly, "u4", Some("gemini-3"), 3);
        assert_eq!(
            combine(&[&a, &b, &against]).unwrap().standing,
            Standing::Disputed
        );
        let editor = judgement(ProofShape::BindOnly, "ed", None, 4);
        let settled = combine(&[&a, &b, &against, &editor]).unwrap();
        assert_eq!(
            (settled.shape, settled.standing),
            (Some(ProofShape::BindOnly), Standing::Confirmed)
        );
        assert_eq!(model_family("kimi-k2"), "kimi");
    }
}
