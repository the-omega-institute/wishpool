//! Map neutral Layer 3 findings onto Layer 2 reports and judgements. Model
//! output is sanitised here; anything that cannot be represented faithfully
//! is dropped and named in the report summary rather than invented.

use wishpool_core::{
    ids::ClaimId,
    model::{
        Claim, ClaimRole, ContributionOutput, Outcome, PriorRelation, PriorWork, ProofShape,
        ReportDraft, Source, SourceKind, StagePayload, TaskKind,
    },
};
use wishpool_review::{
    CandidateRelation, EscapeProposal, JudgementDraft, Statement, openalex::Work,
};

const MAX_WITNESSES: usize = 10;
const MAX_WITNESS_CHARS: usize = 1_000;

/// The proved statements, in the neutral form models read.
pub(crate) fn statements(claims: &[Claim]) -> Vec<Statement> {
    claims
        .iter()
        .filter(|c| !c.kind.is_open())
        .map(|c| Statement {
            id: c.id.to_string(),
            label: c.label.clone(),
            statement: c.statement.clone(),
            main: c.role == ClaimRole::Main,
            proved: !c.kind.is_open(),
            depends_on: c.depends_on.iter().map(ToString::to_string).collect(),
        })
        .collect()
}

/// A work that exists, as a source: its DOI when it has one, else its
/// OpenAlex page.
pub(crate) fn source_of(work: &Work) -> Option<Source> {
    let doi = work
        .doi
        .as_deref()
        .map(|d| d.trim_start_matches("https://doi.org/"))
        .filter(|d| d.starts_with("10.") && d.contains('/'));
    let source = match doi {
        Some(doi) => Source {
            kind: SourceKind::Doi,
            locator: doi.to_owned(),
            year: work.year,
        },
        None => Source {
            kind: SourceKind::Url,
            locator: work.id.clone(),
            year: work.year,
        },
    };
    source.validate().is_ok().then_some(source)
}

fn relation(value: &str) -> Option<PriorRelation> {
    match value {
        "same" => Some(PriorRelation::Same),
        "implies" => Some(PriorRelation::Implies),
        "related" => Some(PriorRelation::Related),
        _ => None,
    }
}

/// What the literature search found for one statement.
pub(crate) struct Found {
    pub claim: ClaimId,
    pub queries: Vec<String>,
    pub candidates: Vec<Work>,
    pub relations: Vec<CandidateRelation>,
}

/// Leads from a search engine, related to the statements by a model. Every
/// lead names a work the search returned; the editor decides.
pub(crate) fn literature(found: &[Found], model: &str) -> ReportDraft {
    let mut prior = Vec::new();
    let mut dropped = 0;
    for f in found {
        for r in &f.relations {
            let (Some(work), Some(relation)) = (f.candidates.get(r.index), relation(&r.relation))
            else {
                dropped += 1;
                continue;
            };
            match source_of(work) {
                Some(source) => prior.push(PriorWork {
                    claim: f.claim.clone(),
                    source,
                    relation,
                    note: format!("{} — {}", work.title, r.note)
                        .chars()
                        .take(2_000)
                        .collect(),
                }),
                None => dropped += 1,
            }
        }
    }
    let mut summary = format!(
        "{} lead(s) among works OpenAlex returned, related by {model}. Leads are proposals; an editor checks each and files the literature report.",
        prior.len()
    );
    if dropped > 0 {
        summary.push_str(&format!(" {dropped} unusable relation(s) dropped."));
    }
    ReportDraft {
        outcome: Outcome::NeedsHuman {
            question: "Check each lead against the statement; file a human literature report."
                .into(),
        },
        summary,
        payload: StagePayload::Literature {
            prior,
            searched: found
                .iter()
                .flat_map(|f| f.queries.iter().map(|q| format!("OpenAlex: {q}")))
                .collect(),
        },
        evidence: vec![],
    }
}

fn witnesses(raw: &[String]) -> Vec<String> {
    raw.iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .take(MAX_WITNESSES)
        .map(|w| w.chars().take(MAX_WITNESS_CHARS).collect())
        .collect()
}

fn rationale(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "(no rationale given)".into()
    } else {
        trimmed.chars().take(10_000).collect()
    }
}

/// A judgement output from one proposal, or `None` when it cannot be
/// represented (content asserted without a witness).
pub(crate) fn judgement(
    content: bool,
    raw_witnesses: &[String],
    raw_rationale: &str,
) -> Option<ContributionOutput> {
    let witnesses = witnesses(raw_witnesses);
    let output = match (content, witnesses.is_empty()) {
        (true, false) => ContributionOutput::Judgement {
            shape: ProofShape::Content,
            witnesses,
            rationale: rationale(raw_rationale),
        },
        (false, _) => ContributionOutput::Judgement {
            shape: ProofShape::BindOnly,
            witnesses: vec![],
            rationale: rationale(raw_rationale),
        },
        (true, true) => return None,
    };
    output
        .validate(TaskKind::JudgeEscape)
        .is_ok()
        .then_some(output)
}

/// Proposals for the paper's proved statements; others are dropped.
pub(crate) fn proposals(
    proposals: &[EscapeProposal],
    claims: &[Claim],
) -> (Vec<(ClaimId, ContributionOutput)>, Vec<String>) {
    let mut kept = Vec::new();
    let mut skipped = Vec::new();
    for p in proposals {
        let Some(claim) = claims
            .iter()
            .find(|c| c.id.as_str() == p.claim && !c.kind.is_open())
        else {
            skipped.push(p.claim.clone());
            continue;
        };
        if kept.iter().any(|(id, _)| id == &claim.id) {
            continue;
        }
        match judgement(p.content, &p.witnesses, &p.rationale) {
            Some(output) => kept.push((claim.id.clone(), output)),
            None => skipped.push(p.claim.clone()),
        }
    }
    (kept, skipped)
}

pub(crate) fn draft_judgement(draft: &JudgementDraft) -> Option<ContributionOutput> {
    match draft.shape.as_str() {
        "content" => judgement(true, &draft.witnesses, &draft.rationale),
        "bind_only" => judgement(false, &[], &draft.rationale),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use wishpool_core::model::ClaimKind;

    use super::*;

    fn claim(id: &str, kind: ClaimKind) -> Claim {
        Claim {
            id: id.into(),
            kind,
            label: id.into(),
            latex_label: None,
            statement: "s".into(),
            role: ClaimRole::Main,
            has_proof: true,
            section: None,
            depends_on: vec![],
            settles: None,
        }
    }

    fn proposal(claim: &str, content: bool, witnesses: &[&str]) -> EscapeProposal {
        EscapeProposal {
            claim: claim.into(),
            content,
            witnesses: witnesses.iter().map(|w| w.to_string()).collect(),
            rationale: "r".into(),
        }
    }

    #[test]
    fn content_without_witness_is_not_invented() {
        let claims = [
            claim("C1", ClaimKind::Theorem),
            claim("C2", ClaimKind::Conjecture),
        ];
        let (kept, skipped) = proposals(
            &[
                proposal("C1", true, &["  "]),
                proposal("C2", false, &[]),
                proposal("C9", false, &[]),
            ],
            &claims,
        );
        assert!(kept.is_empty());
        assert_eq!(skipped, vec!["C1", "C2", "C9"]);
        let (kept, _) = proposals(&[proposal("C1", false, &["stray"])], &claims);
        assert!(
            matches!(&kept[0].1, ContributionOutput::Judgement { shape: ProofShape::BindOnly, witnesses, .. } if witnesses.is_empty())
        );
    }

    #[test]
    fn leads_name_only_returned_works() {
        let work = |id: &str, doi: Option<&str>| Work {
            id: format!("https://openalex.org/{id}"),
            title: "Gaps".into(),
            doi: doi.map(str::to_owned),
            year: Some(2020),
            abstract_text: String::new(),
        };
        let found = Found {
            claim: "C1".into(),
            queries: vec!["gaps".into()],
            candidates: vec![
                work("W1", Some("https://doi.org/10.1000/x")),
                work("W2", None),
            ],
            relations: vec![
                CandidateRelation {
                    index: 0,
                    relation: "implies".into(),
                    note: "n".into(),
                },
                CandidateRelation {
                    index: 1,
                    relation: "related".into(),
                    note: String::new(),
                },
                CandidateRelation {
                    index: 7,
                    relation: "same".into(),
                    note: String::new(),
                },
                CandidateRelation {
                    index: 0,
                    relation: "proves".into(),
                    note: String::new(),
                },
            ],
        };
        let draft = literature(&[found], "m");
        let StagePayload::Literature { prior, searched } = draft.payload else {
            panic!()
        };
        assert_eq!(prior.len(), 2);
        assert_eq!(prior[0].source.locator, "10.1000/x");
        assert_eq!(prior[1].source.kind, SourceKind::Url);
        assert_eq!(searched, vec!["OpenAlex: gaps"]);
        assert!(draft.summary.contains("2 unusable"));
    }
}

pub(crate) fn confirmed_statements(claims: &[Claim]) -> Vec<Statement> {
    claims
        .iter()
        .map(|c| Statement {
            id: c.id.to_string(),
            label: c.label.clone(),
            statement: c.statement.clone(),
            main: c.role == ClaimRole::Main,
            proved: !c.kind.is_open(),
            depends_on: c.depends_on.iter().map(ToString::to_string).collect(),
        })
        .collect()
}

pub(crate) fn referee(
    raw: wishpool_review::referee_prompts::RefereeOut,
    text: String,
    claims: &[Claim],
) -> wishpool_core::model::RefereeReport {
    use wishpool_core::model::{
        Recommendation, RefereeClaim, RefereeConcern, RefereeReport, Severity,
    };
    let recommendation = match raw.recommendation.as_str() {
        "accept" => Some(Recommendation::Accept),
        "minor_revision" => Some(Recommendation::MinorRevision),
        "major_revision" => Some(Recommendation::MajorRevision),
        "reject" => Some(Recommendation::Reject),
        _ => None,
    };
    let proved = |id: &str| {
        claims
            .iter()
            .find(|c| c.id.as_str() == id && !c.kind.is_open())
    };
    let mut limits = raw.limits;
    let mut readings = Vec::new();
    let mut dropped_readings = 0;
    for reading in raw.claims {
        let Some(claim) = proved(&reading.claim) else {
            dropped_readings += 1;
            continue;
        };
        let shape = match reading.shape.as_str() {
            "content" => ProofShape::Content,
            "bind_only" => ProofShape::BindOnly,
            _ => {
                dropped_readings += 1;
                continue;
            }
        };
        let witness: Vec<String> = reading
            .witnesses
            .iter()
            .map(|w| w.trim())
            .filter(|w| !w.is_empty())
            .map(str::to_owned)
            .collect();
        if (shape == ProofShape::Content && witness.is_empty())
            || (shape == ProofShape::BindOnly && !witness.is_empty())
            || readings.iter().any(|r: &RefereeClaim| r.claim == claim.id)
        {
            dropped_readings += 1;
            continue;
        }
        let reading = RefereeClaim {
            claim: claim.id.clone(),
            shape,
            witnesses: witness,
            known: reading.known.filter(|s| !s.trim().is_empty()),
            note: reading.note,
        };
        if referee_judgement(&reading).is_none() {
            dropped_readings += 1;
            continue;
        }
        readings.push(reading);
    }
    let mut concerns = Vec::new();
    let mut dropped_concerns = 0;
    for concern in raw.concerns {
        if concern
            .claim
            .as_ref()
            .is_some_and(|id| proved(id).is_none())
        {
            dropped_concerns += 1;
            continue;
        }
        let severity = match concern.severity.as_str() {
            "major" => Severity::Major,
            "minor" => Severity::Minor,
            _ => {
                dropped_concerns += 1;
                continue;
            }
        };
        concerns.push(RefereeConcern {
            claim: concern.claim.map(ClaimId),
            severity,
            issue: concern.issue,
        });
    }
    if dropped_readings > 0 {
        limits.push(format!(
            "{dropped_readings} unusable, duplicate, unknown or open statement reading(s) dropped."
        ));
    }
    if dropped_concerns > 0 {
        limits.push(format!("{dropped_concerns} unusable concern(s) or concerns naming unknown or open statements dropped."));
    }
    if recommendation.is_none() {
        limits.push(
            "The recommendation was not recognised; no positive recommendation recorded.".into(),
        );
    }
    RefereeReport {
        recommendation,
        summary: raw.summary,
        strengths: raw.strengths,
        concerns,
        claims: readings,
        limits,
        text,
    }
}

/// Preserve the mathematical witness and rationale verbatim. If the domain
/// cannot hold them, omit the reading with a limit rather than truncating it.
pub(crate) fn referee_judgement(
    reading: &wishpool_core::model::RefereeClaim,
) -> Option<ContributionOutput> {
    let note = match &reading.known {
        Some(source) => format!(
            "{}\nKnown source (referee's report): {source}",
            reading.note
        ),
        None => reading.note.clone(),
    };
    let note = if note.trim().is_empty() {
        "(no rationale given)".into()
    } else {
        note
    };
    let output = ContributionOutput::Judgement {
        shape: reading.shape,
        witnesses: reading.witnesses.clone(),
        rationale: note,
    };
    output
        .validate(TaskKind::JudgeEscape)
        .is_ok()
        .then_some(output)
}

pub(crate) fn referee_out(
    report: &wishpool_core::model::RefereeReport,
) -> wishpool_review::referee_prompts::RefereeOut {
    // The domain report has the same JSON vocabulary after sanitisation.
    serde_json::from_value(serde_json::to_value(report).unwrap_or_default()).unwrap_or_default()
}

pub(crate) fn advice(
    raw: wishpool_review::referee_prompts::AdviceOut,
    claims: &[Claim],
) -> wishpool_core::model::Advice {
    use wishpool_core::model::{
        Advice, Effort, Feasibility, FormalizationCandidate, Improvement,
        ImprovementEvidence as Evidence, ImprovementKind,
    };
    let effort = |value: &str| match value {
        "small" => Some(Effort::Small),
        "medium" => Some(Effort::Medium),
        "large" => Some(Effort::Large),
        _ => None,
    };
    let mut improvements = Vec::new();
    let mut formalization = Vec::new();
    let mut dropped = 0;
    let mut omitted_proposal_evidence = 0;
    for item in raw.improvements {
        if item
            .claim
            .as_ref()
            .is_some_and(|id| !claims.iter().any(|c| c.id.as_str() == id))
        {
            dropped += 1;
            continue;
        }
        let kind = match item.kind.as_str() {
            "gap" => ImprovementKind::Gap,
            "strengthen" => ImprovementKind::Strengthen,
            "generalize" => ImprovementKind::Generalize,
            "computation" => ImprovementKind::Computation,
            "literature" => ImprovementKind::Literature,
            "exposition" => ImprovementKind::Exposition,
            _ => {
                dropped += 1;
                continue;
            }
        };
        let Some(effort) = effort(&item.effort) else {
            dropped += 1;
            continue;
        };
        let status = match item.status.as_str() {
            "checked" if !item.evidence.trim().is_empty() => Evidence::Checked,
            "proposed" => Evidence::Proposed,
            _ => {
                dropped += 1;
                continue;
            }
        };
        if status == Evidence::Proposed && !item.evidence.is_empty() {
            omitted_proposal_evidence += 1;
        }
        improvements.push(Improvement {
            claim: item.claim.map(ClaimId),
            kind,
            suggestion: item.suggestion,
            how_we_help: item.how_we_help,
            effort,
            status,
            evidence: if status == Evidence::Checked {
                item.evidence
            } else {
                String::new()
            },
        });
    }
    for item in raw.formalization {
        let Some(claim) = claims
            .iter()
            .find(|c| c.id.as_str() == item.claim && !c.kind.is_open())
        else {
            dropped += 1;
            continue;
        };
        let feasibility = match item.feasibility.as_str() {
            "ready" => Feasibility::Ready,
            "needs_library" => Feasibility::NeedsLibrary,
            "hard" => Feasibility::Hard,
            _ => {
                dropped += 1;
                continue;
            }
        };
        let Some(effort) = effort(&item.effort) else {
            dropped += 1;
            continue;
        };
        formalization.push(FormalizationCandidate {
            claim: claim.id.clone(),
            feasibility,
            mathlib: item.mathlib,
            missing: item.missing,
            lean_sketch: item.lean_sketch,
            plan: item.plan,
            effort,
        });
    }
    let mut summary = raw.summary;
    if dropped > 0 {
        summary.push_str(&format!(
            " {dropped} unusable improvement or formalization candidate(s) dropped."
        ));
    }
    if omitted_proposal_evidence > 0 {
        summary.push_str(&format!(" {omitted_proposal_evidence} proposal evidence field(s) omitted: proposals do not record completed checks."));
    }
    Advice {
        summary,
        improvements,
        formalization,
    }
}

pub(crate) fn advice_out(
    advice: &wishpool_core::model::Advice,
) -> wishpool_review::referee_prompts::AdviceOut {
    serde_json::from_value(serde_json::to_value(advice).unwrap_or_default()).unwrap_or_default()
}

/// Statements formalization is attempted on: proved statements the advice
/// did not call hard, most tractable first.
pub(crate) const MAX_FORMAL_TARGETS: usize = 2;

pub(crate) fn formal_targets(
    advice: &wishpool_core::model::Advice,
    claims: &[Claim],
) -> Vec<wishpool_review::lean::FormalTarget> {
    use wishpool_core::model::{Effort, Feasibility};
    let rank = |f: Feasibility| match f {
        Feasibility::Ready => 0,
        Feasibility::NeedsLibrary => 1,
        Feasibility::Hard => 2,
    };
    let cost = |e: Effort| match e {
        Effort::Small => 0,
        Effort::Medium => 1,
        Effort::Large => 2,
    };
    let mut candidates: Vec<_> = advice
        .formalization
        .iter()
        .filter(|f| f.feasibility != Feasibility::Hard)
        .filter_map(|f| {
            claims
                .iter()
                .find(|c| c.id == f.claim && !c.kind.is_open())
                .map(|c| (f, c))
        })
        .collect();
    candidates.sort_by_key(|(f, _)| (rank(f.feasibility), cost(f.effort)));
    candidates.dedup_by(|a, b| a.1.id == b.1.id);
    candidates
        .into_iter()
        .take(MAX_FORMAL_TARGETS)
        .map(|(f, c)| wishpool_review::lean::FormalTarget {
            claim: c.id.to_string(),
            label: c.label.clone(),
            statement: c.statement.clone(),
            lean_sketch: f.lean_sketch.clone(),
            plan: f.plan.clone(),
            mathlib: f.mathlib.clone(),
        })
        .collect()
}

/// Every target gets an attempt; only files this binary checked count.
pub(crate) fn formal(
    out: wishpool_review::lean::FormalOut,
    targets: &[wishpool_review::lean::FormalTarget],
) -> wishpool_core::model::FormalProbe {
    use wishpool_core::model::{FormalAttempt, FormalProbe, ProbeOutcome};
    let attempts = targets
        .iter()
        .map(
            |target| match out.files.iter().find(|f| f.claim == target.claim) {
                Some(file) => FormalAttempt {
                    claim: ClaimId(target.claim.clone()),
                    outcome: if file.compiled {
                        ProbeOutcome::Compiled
                    } else {
                        ProbeOutcome::Failed
                    },
                    theorem: file.theorem.clone(),
                    lean: file.lean.clone(),
                    axioms: file.axioms.clone(),
                    note: file.note.clone(),
                    log: file.log.clone(),
                },
                None => FormalAttempt {
                    claim: ClaimId(target.claim.clone()),
                    outcome: ProbeOutcome::Failed,
                    theorem: None,
                    lean: String::new(),
                    axioms: vec![],
                    note: String::new(),
                    log: "no Lean file was written".into(),
                },
            },
        )
        .collect();
    FormalProbe {
        toolchain: out.toolchain,
        attempts,
        summary: out.summary,
    }
}

pub(crate) fn letter(
    raw: wishpool_review::referee_prompts::LetterOut,
) -> wishpool_review::ReviewResult<wishpool_core::model::LetterDraft> {
    use wishpool_core::model::{LetterDraft, MAX_LETTER_CHARS};
    if raw.body.trim().is_empty()
        || raw.body.chars().count() > MAX_LETTER_CHARS
        || raw.note.chars().count() > MAX_LETTER_CHARS
        || raw.subject.chars().count() > 300
    {
        return Err(wishpool_review::ReviewError::Output(
            "letter is empty or exceeds its length limits".into(),
        ));
    }
    Ok(LetterDraft {
        subject: raw.subject,
        body: raw.body,
        note: raw.note,
    })
}

#[cfg(test)]
mod referee_tests {
    use super::*;
    use wishpool_core::model::{ClaimKind, ImprovementEvidence, Recommendation};
    use wishpool_review::referee_prompts::{
        AdviceOut, ConcernOut, FormalizationOut, ImprovementOut, LetterOut, ReadingOut, RefereeOut,
    };

    fn claims() -> Vec<Claim> {
        [ClaimKind::Theorem, ClaimKind::Conjecture]
            .into_iter()
            .enumerate()
            .map(|(i, kind)| Claim {
                id: format!("C{}", i + 1).as_str().into(),
                kind,
                label: "label".into(),
                latex_label: None,
                statement: "s".into(),
                role: ClaimRole::Main,
                has_proof: !kind.is_open(),
                section: None,
                depends_on: vec![],
                settles: None,
            })
            .collect()
    }

    #[test]
    fn referee_sanitises_ids_shapes_and_preserves_raw_text() {
        let raw = RefereeOut {
            recommendation: "accept".into(),
            claims: vec![
                ReadingOut {
                    claim: "C1".into(),
                    shape: "content".into(),
                    witnesses: vec!["Lemma".into()],
                    ..Default::default()
                },
                ReadingOut {
                    claim: "C2".into(),
                    shape: "bind_only".into(),
                    ..Default::default()
                },
                ReadingOut {
                    claim: "C99".into(),
                    shape: "bind_only".into(),
                    ..Default::default()
                },
            ],
            concerns: vec![
                ConcernOut {
                    claim: Some("C2".into()),
                    severity: "major".into(),
                    ..Default::default()
                },
                ConcernOut {
                    severity: "minor".into(),
                    issue: "Paper-wide".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let report = referee(raw, "original answer".into(), &claims());
        assert_eq!(report.recommendation, Some(Recommendation::Accept));
        assert_eq!(report.claims.len(), 1);
        assert_eq!(report.concerns.len(), 1);
        assert_eq!(report.text, "original answer");
        assert!(report.limits.iter().any(|s| s.starts_with("2 unusable")));
        assert!(report.limits.iter().any(|s| s.starts_with("1 unusable")));
        assert_eq!(referee_out(&report).recommendation, "accept");
        assert_eq!(referee_out(&report).text, "original answer");
        assert_eq!(
            referee(
                RefereeOut {
                    recommendation: "excellent".into(),
                    ..Default::default()
                },
                "".into(),
                &claims()
            )
            .recommendation,
            None
        );
    }

    #[test]
    fn advice_drops_unproved_candidates_and_unevidenced_checks() {
        let raw = AdviceOut {
            improvements: vec![
                ImprovementOut {
                    claim: Some("C1".into()),
                    kind: "gap".into(),
                    effort: "small".into(),
                    status: "checked".into(),
                    ..Default::default()
                },
                ImprovementOut {
                    kind: "exposition".into(),
                    effort: "small".into(),
                    status: "proposed".into(),
                    evidence: "a proposal is not evidence".into(),
                    ..Default::default()
                },
                ImprovementOut {
                    kind: "strengthen".into(),
                    effort: "medium".into(),
                    status: "checked".into(),
                    evidence: "The full argument: ...".into(),
                    ..Default::default()
                },
            ],
            formalization: vec![
                FormalizationOut {
                    claim: "C2".into(),
                    feasibility: "ready".into(),
                    effort: "small".into(),
                    ..Default::default()
                },
                FormalizationOut {
                    claim: "C1".into(),
                    feasibility: "needs_library".into(),
                    effort: "medium".into(),
                    lean_sketch: "theorem example_statement : True := by trivial".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mapped = advice(raw, &claims());
        assert_eq!(mapped.improvements.len(), 2);
        assert_eq!(mapped.improvements[0].status, ImprovementEvidence::Proposed);
        assert!(mapped.improvements[0].evidence.is_empty());
        assert_eq!(mapped.improvements[1].status, ImprovementEvidence::Checked);
        assert_eq!(mapped.formalization.len(), 1);
        assert!(!mapped.formalization[0].lean_sketch.is_empty());
        assert!(mapped.summary.contains("2 unusable"));
        assert!(mapped.summary.contains("1 proposal evidence field"));
        assert_eq!(advice_out(&mapped).improvements[1].status, "checked");
    }

    #[test]
    fn letter_validation_does_not_truncate_mathematics() {
        assert!(letter(LetterOut::default()).is_err());
        assert!(
            letter(LetterOut {
                body: "x".repeat(50_001),
                ..Default::default()
            })
            .is_err()
        );
        let draft = letter(LetterOut {
            subject: "Feedback".into(),
            body: "Point".into(),
            note: "$x=1$".into(),
        })
        .unwrap();
        assert_eq!(draft.note, "$x=1$");
    }
}

#[cfg(test)]
mod nonadjacent_proof_tests {
    use super::*;
    use wishpool_review::referee_prompts::{ReadingOut, RefereeOut};

    #[test]
    fn theorem_with_a_nonadjacent_proof_remains_reviewable() {
        let claim = Claim {
            id: "C1".into(),
            kind: wishpool_core::model::ClaimKind::Theorem,
            label: "Theorem 1".into(),
            latex_label: None,
            statement: "A result whose proof is in an appendix".into(),
            role: ClaimRole::Main,
            has_proof: false,
            section: None,
            depends_on: vec![],
            settles: None,
        };
        assert!(confirmed_statements(std::slice::from_ref(&claim))[0].proved);
        let raw = RefereeOut {
            claims: vec![ReadingOut {
                claim: "C1".into(),
                shape: "bind_only".into(),
                note: "Proof in Appendix A".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(referee(raw, "answer".into(), &[claim]).claims.len(), 1);
    }
}

#[cfg(test)]
mod referee_fidelity_tests {
    use super::*;
    use wishpool_core::model::RefereeClaim;

    #[test]
    fn referee_witness_and_rationale_are_not_truncated() {
        let reading = RefereeClaim {
            claim: "C1".into(),
            shape: ProofShape::Content,
            witnesses: vec!["x".repeat(1_500)],
            known: Some("A reported source".into()),
            note: "An argument".into(),
        };
        let output = referee_judgement(&reading).unwrap();
        assert!(
            matches!(output, ContributionOutput::Judgement { witnesses, rationale, .. }
            if witnesses[0].len() == 1_500 && rationale == "An argument\nKnown source (referee's report): A reported source")
        );
        let reading = RefereeClaim {
            note: "x".repeat(10_001),
            ..reading
        };
        assert!(referee_judgement(&reading).is_none());
    }
}
