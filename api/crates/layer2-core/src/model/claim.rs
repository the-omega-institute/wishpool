//! The statements of a paper and what is known about each.

use serde::{Deserialize, Serialize};

use super::common::{Source, require_text};
use crate::{CoreError, CoreResult, ids::ClaimId};

/// The kind of a statement, as the paper declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Theorem,
    Proposition,
    Lemma,
    Corollary,
    Claim,
    /// Stated without proof as expected to hold.
    Conjecture,
    /// An open question posed by the paper.
    Question,
}

impl ClaimKind {
    /// Conjectures and questions are open: the paper does not prove them.
    pub fn is_open(self) -> bool {
        matches!(self, Self::Conjecture | Self::Question)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimRole {
    /// A headline result. The publication threshold is decided over main
    /// results.
    Main,
    Supporting,
}

/// A named, sourced open problem a claim says it settles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenProblemRef {
    pub name: String,
    pub source: Source,
}

/// One statement of the paper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    /// `C1`, `C2`, … in order of appearance.
    pub id: ClaimId,
    pub kind: ClaimKind,
    /// How a reader finds it: "Theorem (Main estimate)", "Lemma [lem:gap]".
    pub label: String,
    /// The `\label` in the source, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latex_label: Option<String>,
    /// The statement as LaTeX.
    pub statement: String,
    pub role: ClaimRole,
    /// A proof follows the statement in the paper.
    #[serde(default)]
    pub has_proof: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<ClaimId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settles: Option<OpenProblemRef>,
}

impl Claim {
    /// A proved main result: what the publication threshold looks at.
    pub fn is_main_result(&self) -> bool {
        self.role == ClaimRole::Main && !self.kind.is_open()
    }
}

pub fn validate_claims(claims: &[Claim]) -> CoreResult<()> {
    if claims.is_empty() {
        return Err(CoreError::invalid("the paper has no statements to analyse"));
    }
    if claims.len() > 400 {
        return Err(CoreError::invalid("at most 400 statements"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for claim in claims {
        require_text("claim id", claim.id.as_str(), 32)?;
        require_text("claim label", &claim.label, 300)?;
        require_text("claim statement", &claim.statement, 40_000)?;
        if !seen.insert(&claim.id) {
            return Err(CoreError::invalid(format!(
                "duplicate claim id {}",
                claim.id
            )));
        }
        if let Some(problem) = &claim.settles {
            require_text("open problem name", &problem.name, 300)?;
            problem.source.validate()?;
        }
    }
    for claim in claims {
        for dependency in &claim.depends_on {
            if dependency == &claim.id || !seen.contains(dependency) {
                return Err(CoreError::invalid(format!(
                    "claim {} depends on unknown claim {dependency}",
                    claim.id
                )));
            }
        }
    }
    if has_cycle(claims) {
        return Err(CoreError::invalid("claim dependencies contain a cycle"));
    }
    Ok(())
}

fn has_cycle(claims: &[Claim]) -> bool {
    use std::collections::BTreeMap;
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Visiting,
        Done,
    }
    fn visit<'a>(
        node: &'a ClaimId,
        edges: &BTreeMap<&'a ClaimId, &'a Vec<ClaimId>>,
        marks: &mut BTreeMap<&'a ClaimId, Mark>,
    ) -> bool {
        match marks.get(node) {
            Some(Mark::Done) => return false,
            Some(Mark::Visiting) => return true,
            None => {}
        }
        marks.insert(node, Mark::Visiting);
        if let Some(next) = edges.get(node)
            && next.iter().any(|d| visit(d, edges, marks))
        {
            return true;
        }
        marks.insert(node, Mark::Done);
        false
    }
    let edges: BTreeMap<&ClaimId, &Vec<ClaimId>> =
        claims.iter().map(|c| (&c.id, &c.depends_on)).collect();
    let mut marks = BTreeMap::new();
    claims.iter().any(|c| visit(&c.id, &edges, &mut marks))
}

/// Proof shape of a claim relative to prior results. A claim is bind-only
/// when its conclusion follows from known results by instantiation,
/// projection, regrouping or normalising rewrites alone; otherwise it
/// carries content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofShape {
    BindOnly,
    Content,
}

/// An exact fraction; escape rates are reported only as exact readings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ratio {
    pub numerator: u64,
    pub denominator: u64,
}

impl Ratio {
    pub fn validate(&self) -> CoreResult<()> {
        if self.denominator == 0 || self.numerator > self.denominator {
            return Err(CoreError::invalid("ratio must satisfy 0 <= n <= d, d > 0"));
        }
        Ok(())
    }

    pub fn le(&self, other: &Ratio) -> bool {
        u128::from(self.numerator) * u128::from(other.denominator)
            <= u128::from(other.numerator) * u128::from(self.denominator)
    }
}

/// A measured escape-rate reading on an explicitly built finite arena,
/// before and after the claim. Present only when the arena was built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscapeRateReading {
    pub arena: String,
    pub before: Ratio,
    pub after: Ratio,
    pub artifact: String,
}

impl EscapeRateReading {
    pub fn validate(&self) -> CoreResult<()> {
        require_text("arena", &self.arena, 2_000)?;
        require_text("artifact", &self.artifact, 1_000)?;
        self.before.validate()?;
        self.after.validate()?;
        if !self.after.le(&self.before) {
            return Err(CoreError::invalid(
                "escape rate cannot increase after adding a result",
            ));
        }
        Ok(())
    }
}

/// The escape judgement of one claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscapeAssessment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conjecture: Option<super::ConjectureReading>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correctness: Option<super::referee::Correctness>,
    pub claim: ClaimId,
    pub shape: ProofShape,
    /// New propositions on the live proof path that prior results do not
    /// give by binding alone.
    #[serde(default)]
    pub witnesses: Vec<String>,
    pub rationale: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escape_rate: Option<EscapeRateReading>,
}

impl EscapeAssessment {
    pub fn validate(&self) -> CoreResult<()> {
        require_text("rationale", &self.rationale, 10_000)?;
        match self.shape {
            ProofShape::Content if self.witnesses.is_empty() && self.conjecture.is_none() => {
                Err(CoreError::invalid(format!(
                    "content claim {} must name at least one escape witness",
                    self.claim
                )))
            }
            ProofShape::BindOnly if !self.witnesses.is_empty() => Err(CoreError::invalid(format!(
                "bind-only claim {} cannot list escape witnesses",
                self.claim
            ))),
            _ => Ok(()),
        }?;
        if let Some(reading) = &self.escape_rate {
            reading.validate()?;
        }
        Ok(())
    }

    pub fn has_escape_content(&self) -> bool {
        self.correctness
            .is_none_or(|c| c == super::referee::Correctness::Correct)
            && self.shape == ProofShape::Content
            && !self.witnesses.is_empty()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn claim(id: &str, kind: ClaimKind, role: ClaimRole, deps: &[&str]) -> Claim {
        Claim {
            id: id.into(),
            kind,
            label: format!("{kind:?} {id}"),
            latex_label: None,
            statement: "s".into(),
            role,
            has_proof: true,
            section: None,
            depends_on: deps.iter().map(|d| ClaimId::from(*d)).collect(),
            settles: None,
        }
    }

    #[test]
    fn validation() {
        let main = claim("C1", ClaimKind::Theorem, ClaimRole::Main, &[]);
        let lemma = claim("C2", ClaimKind::Lemma, ClaimRole::Supporting, &["C1"]);
        assert!(validate_claims(&[main.clone(), lemma]).is_ok());
        let a = claim("C1", ClaimKind::Lemma, ClaimRole::Main, &["C2"]);
        let b = claim("C2", ClaimKind::Lemma, ClaimRole::Main, &["C1"]);
        assert!(validate_claims(&[a, b]).is_err());
        assert!(validate_claims(&[]).is_err());
        assert!(main.is_main_result());
        assert!(!claim("C3", ClaimKind::Conjecture, ClaimRole::Main, &[]).is_main_result());
    }

    #[test]
    fn escape_readings_cannot_increase() {
        let reading = |b: u64, a: u64| EscapeRateReading {
            arena: "a".into(),
            before: Ratio {
                numerator: b,
                denominator: 12,
            },
            after: Ratio {
                numerator: a,
                denominator: 12,
            },
            artifact: "https://example.org/x".into(),
        };
        assert!(reading(6, 2).validate().is_ok());
        assert!(reading(2, 6).validate().is_err());
    }
}
