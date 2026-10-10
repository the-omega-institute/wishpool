//! Map neutral Layer 3 findings onto Layer 2 reports and judgements. Model
//! output is sanitised here; anything that cannot be represented faithfully
//! is dropped and named in the report summary rather than invented.

use wishpool_core::{
    ids::ClaimId,
    model::{Claim, ClaimRole, ContributionOutput, ProofShape, TaskKind},
};
use wishpool_review::{EscapeProposal, JudgementDraft, Statement};

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

fn conjecture_reading(
    raw: wishpool_review::referee_prompts::ConjectureOut,
    named: Option<&str>,
) -> Option<wishpool_core::model::ConjectureReading> {
    use wishpool_core::model::{ConjectureReading, ConjectureStatus};
    let well_posed = raw.well_posed?;
    for reason in [
        &raw.well_posed_reason,
        &raw.status_reason,
        &raw.escape_reason,
    ] {
        if reason.trim().is_empty() || reason.chars().count() > 8_000 {
            return None;
        }
    }
    let mut status = match raw.status.as_str() {
        "open" => ConjectureStatus::Open,
        "known_true" => ConjectureStatus::KnownTrue,
        "known_false" => ConjectureStatus::KnownFalse,
        "special_case_of_known" => ConjectureStatus::SpecialCaseOfKnown,
        "unclear" => ConjectureStatus::Unclear,
        _ => return None,
    };
    if raw.named_works.len() > 20
        || raw.named_works.iter().any(|w| {
            w.trim().is_empty() || w.chars().count() > 2_000 || !super::sources::reported_work(w)
        })
    {
        return None;
    }
    if named.is_some()
        && matches!(
            status,
            ConjectureStatus::KnownTrue
                | ConjectureStatus::KnownFalse
                | ConjectureStatus::SpecialCaseOfKnown
        )
        && (!matches!(raw.status_basis.as_str(), "offline" | "opened_source")
            || raw.status_evidence.trim().is_empty()
            || raw.status_basis == "opened_source" && raw.named_works.is_empty())
    {
        status = ConjectureStatus::Unclear;
    }
    let escape = match raw.escape.as_str() {
        "content" => ProofShape::Content,
        "bind_only" => ProofShape::BindOnly,
        _ => return None,
    };
    if raw.suggestions.len() > 20 || raw.suggestions.iter().any(|s| s.chars().count() > 5_000) {
        return None;
    }
    Some(ConjectureReading {
        well_posed,
        well_posed_reason: raw.well_posed_reason,
        status,
        status_reason: raw.status_reason,
        named_works: raw.named_works,
        escape,
        escape_reason: raw.escape_reason,
        suggestions: raw.suggestions,
    })
}
pub(crate) fn referee(
    raw: wishpool_review::referee_prompts::RefereeOut,
    text: String,
    claims: &[Claim],
) -> wishpool_core::model::RefereeReport {
    referee_for_kind(raw, text, claims, false)
}

pub(crate) fn referee_for_kind(
    raw: wishpool_review::referee_prompts::RefereeOut,
    text: String,
    claims: &[Claim],
    conjecture: bool,
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
            .find(|c| c.id.as_str() == id && (!c.kind.is_open() || conjecture))
    };
    let mut limits = raw.limits;
    let mut readings = Vec::new();
    let mut dropped_readings = 0;
    let mut dropped_sources = 0;
    for reading in raw.claims {
        let Some(claim) = proved(&reading.claim) else {
            dropped_readings += 1;
            continue;
        };
        if claim.kind.is_open() {
            let Some(checked) = reading.conjecture.and_then(|r| conjecture_reading(r, None)) else {
                dropped_readings += 1;
                continue;
            };
            if readings.iter().any(|r: &RefereeClaim| r.claim == claim.id) {
                dropped_readings += 1;
                continue;
            }
            readings.push(RefereeClaim {
                claim: claim.id.clone(),
                shape: checked.escape,
                witnesses: vec![],
                known: None,
                note: String::new(),
                conjecture: Some(checked),
            });
            continue;
        }
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
        let known = reading.known.filter(|s| {
            let usable = super::sources::reported_work(s);
            if !usable {
                dropped_sources += 1;
            }
            usable
        });
        let reading = RefereeClaim {
            conjecture: None,
            claim: claim.id.clone(),
            shape,
            witnesses: witness,
            known,
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
    if dropped_sources > 0 {
        limits.push(format!(
            "{dropped_sources} named prior work(s) without a usable reported opened source dropped."
        ));
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
    if reading.conjecture.is_some() {
        return None;
    }
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

/// Model output is untrusted: no invented ids, witnesses or citations.
/// Missing/unusable statement readings become explicit not_checked readings.
pub(crate) fn audit(
    raw: wishpool_review::referee_prompts::AuditOut,
    input: &wishpool_review::advisor::AdvisorInput,
    claims: &[Claim],
) -> wishpool_review::ReviewResult<wishpool_core::model::RefereeAudit> {
    use wishpool_core::model::{
        AuditedClaim, AuditedConcern, ConcernStatus, Correctness, Recommendation, RefereeAudit,
    };
    let verdict = match raw.verdict.as_str() {
        "accept" => Recommendation::Accept,
        "minor_revision" => Recommendation::MinorRevision,
        "major_revision" => Recommendation::MajorRevision,
        "reject" => Recommendation::Reject,
        _ => {
            return Err(wishpool_review::ReviewError::Output(
                "unrecognised audit verdict".into(),
            ));
        }
    };
    if raw.summary.trim().is_empty() || raw.summary.chars().count() > 18_000 {
        return Err(wishpool_review::ReviewError::Output(
            "unusable audit summary".into(),
        ));
    }
    let named = format!(
        "{}\n{}",
        serde_json::to_string(&input.referee).unwrap_or_default(),
        input.text.as_deref().unwrap_or_default()
    );
    let mut dropped = 0;
    let mut readings = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for r in raw.claims {
        let Some(claim) = claims.iter().find(|c| c.id.as_str() == r.claim) else {
            dropped += 1;
            continue;
        };
        if !seen.insert(claim.id.clone()) {
            readings.retain(|a: &AuditedClaim| a.claim != claim.id);
            dropped += 1;
            continue;
        }
        if r.comment.trim().is_empty() || r.comment.chars().count() > 8_000 {
            dropped += 1;
            continue;
        }
        if input.kind == "conjecture" && claim.kind.is_open() {
            let Some(checked) = r
                .conjecture
                .and_then(|r| conjecture_reading(r, Some(&named)))
            else {
                dropped += 1;
                continue;
            };
            readings.push(AuditedClaim {
                claim: claim.id.clone(),
                correctness: Correctness::NotChecked,
                comment: r.comment,
                shape: None,
                witnesses: vec![],
                known: None,
                referee_agreed: r.referee_agreed,
                conjecture: Some(checked),
            });
            continue;
        }
        let correctness = match r.correctness.as_str() {
            "correct" if !claim.kind.is_open() => Correctness::Correct,
            "gap" if !claim.kind.is_open() => Correctness::Gap,
            "error" if !claim.kind.is_open() => Correctness::Error,
            "not_checked" => Correctness::NotChecked,
            _ => {
                dropped += 1;
                continue;
            }
        };
        let shape = match r.shape.as_deref() {
            None => None,
            Some("content") if !claim.kind.is_open() => Some(ProofShape::Content),
            Some("bind_only") if !claim.kind.is_open() => Some(ProofShape::BindOnly),
            _ => {
                dropped += 1;
                continue;
            }
        };
        let witnesses: Vec<String> = r
            .witnesses
            .into_iter()
            .filter(|w| !w.trim().is_empty())
            .collect();
        if (correctness == Correctness::Correct && shape.is_none())
            || (shape == Some(ProofShape::Content) && witnesses.is_empty())
            || (shape != Some(ProofShape::Content) && !witnesses.is_empty())
            || witnesses.len() > 20
            || witnesses.iter().any(|w| w.chars().count() > 10_000)
        {
            dropped += 1;
            continue;
        }
        let known = r.known.filter(|k| !k.trim().is_empty());
        if known
            .as_ref()
            .is_some_and(|k| !super::sources::reported_work(k))
        {
            dropped += 1;
            continue;
        }
        readings.push(AuditedClaim {
            conjecture: None,
            claim: claim.id.clone(),
            correctness,
            comment: r.comment,
            shape,
            witnesses,
            known,
            referee_agreed: r.referee_agreed,
        });
    }
    let mut missing = 0;
    for claim in claims {
        if !readings.iter().any(|r| r.claim == claim.id) {
            missing += 1;
            readings.push(AuditedClaim {
                conjecture: None,
                claim: claim.id.clone(),
                correctness: Correctness::NotChecked,
                comment: "No usable source-based check was returned for this statement.".into(),
                shape: None,
                witnesses: vec![],
                known: None,
                referee_agreed: false,
            });
        }
    }
    let mut concerns = Vec::new();
    const OPENED_EVIDENCE: &str = "\nReported opened-source evidence (not venue-verified): ";
    for c in raw.concerns {
        if c.concern.trim().is_empty()
            || c.note.trim().is_empty()
            || c.concern.chars().count() > 5_000
            || c.note.chars().count() > 10_000
            || c.evidence.chars().count() > 8_000
            || c.basis == "opened_source"
                && c.note.chars().count()
                    + c.evidence.chars().count()
                    + OPENED_EVIDENCE.chars().count()
                    > 10_000
        {
            dropped += 1;
            continue;
        }
        let status = match c.status.as_str() {
            "confirmed" | "refuted" if c.basis == "offline" && !c.evidence.trim().is_empty() => {
                // Attribution/search claims cannot be turned into offline evidence.
                let text = format!("{} {}", c.concern, c.note).to_lowercase();
                if [
                    "oeis",
                    "attribution",
                    "doi",
                    "citation",
                    "published",
                    "literature",
                    "arxiv",
                ]
                .iter()
                .any(|word| text.contains(word))
                {
                    ConcernStatus::NotCheckable
                } else if c.status == "confirmed" {
                    ConcernStatus::Confirmed
                } else {
                    ConcernStatus::Refuted
                }
            }
            "confirmed" | "refuted"
                if c.basis == "opened_source"
                    && !c.evidence.trim().is_empty()
                    && c.evidence.lines().any(super::sources::reported_work)
                    && c.evidence.lines().any(|line| {
                        !line.trim().is_empty() && !super::sources::reported_work(line)
                    }) =>
            {
                if c.status == "confirmed" {
                    ConcernStatus::Confirmed
                } else {
                    ConcernStatus::Refuted
                }
            }
            "confirmed" | "refuted" | "not_checkable" => ConcernStatus::NotCheckable,
            _ => {
                dropped += 1;
                continue;
            }
        };
        concerns.push(AuditedConcern {
            concern: c.concern,
            status,
            note: if c.basis == "opened_source" {
                format!("{}{OPENED_EVIDENCE}{}", c.note, c.evidence)
            } else {
                c.note
            },
        });
    }
    let mut summary = raw.summary;
    if missing > 0 {
        summary.push_str(&format!(
            " {missing} statements had no usable check and were marked not checked."
        ));
    }
    if dropped > 0 {
        summary.push_str(&format!(
            " {dropped} unusable readings, concerns or unsupported sources were dropped."
        ));
    }
    Ok(RefereeAudit {
        verdict,
        agrees_with_referee: raw.agrees_with_referee,
        summary,
        claims: readings,
        concerns,
    })
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

    pub(super) fn claims() -> Vec<Claim> {
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
    fn referee_reports_omission_of_names_without_opened_sources() {
        let report = referee(
            RefereeOut {
                claims: vec![ReadingOut {
                    claim: "C1".into(),
                    shape: "bind_only".into(),
                    known: Some("Unsourced attribution".into()),
                    note: "A matching argument".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            String::new(),
            &claims(),
        );
        assert_eq!(report.claims.len(), 1);
        assert!(report.claims[0].known.is_none());
        assert!(
            report
                .limits
                .iter()
                .any(|limit| limit.contains("opened source dropped"))
        );
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
            conjecture: None,
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

#[cfg(test)]
mod audit_tests {
    use super::*;
    use wishpool_core::model::{ConcernStatus, Correctness};
    use wishpool_review::{
        advisor::AdvisorInput,
        referee_prompts::{AuditOut, AuditedClaimOut, AuditedConcernOut, RefereeOut},
    };

    #[test]
    fn audit_drops_invented_sources_ids_and_witnesses_and_marks_offline_limits() {
        let claims = super::referee_tests::claims();
        let input = AdvisorInput {
            kind: "paper".into(),
            title: "Paper".into(),
            abstract_text: String::new(),
            authors: vec![],
            statements: confirmed_statements(&claims),
            referee: RefereeOut {
                text: "A named earlier work [opened: https://example.test/earlier-work]".into(),
                ..Default::default()
            },
            text: Some("The paper cites A source in the paper.".into()),
            source_dir: None,
            main_file: None,
            audit: None,
            decision: None,
        };
        let out = AuditOut {
            verdict: "accept".into(),
            summary: "A source-based check.".into(),
            claims: vec![
                AuditedClaimOut {
                    claim: "C1".into(),
                    correctness: "correct".into(),
                    comment: "A check.".into(),
                    shape: Some("content".into()),
                    witnesses: vec!["An explicit new intermediate identity".into()],
                    known: Some("Invented paper".into()),
                    ..Default::default()
                },
                AuditedClaimOut {
                    claim: "C2".into(),
                    correctness: "correct".into(),
                    comment: "No witness.".into(),
                    shape: Some("content".into()),
                    ..Default::default()
                },
                AuditedClaimOut {
                    claim: "C99".into(),
                    correctness: "correct".into(),
                    ..Default::default()
                },
            ],
            concerns: vec![
                AuditedConcernOut {
                    concern: "The OEIS attribution is correct".into(),
                    status: "confirmed".into(),
                    note: "Attribution claimed in the report".into(),
                    evidence: "Read the referee".into(),
                    basis: "offline".into(),
                },
                AuditedConcernOut {
                    concern: "The base case is wrong".into(),
                    status: "refuted".into(),
                    note: "At n = 1 both sides are 1".into(),
                    evidence: "Substituting 1 yields 1 = 1".into(),
                    basis: "offline".into(),
                },
                AuditedConcernOut {
                    concern: "Missing proof".into(),
                    status: "confirmed".into(),
                    note: "Not checked".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let result = audit(out, &input, &claims).unwrap();
        assert_eq!(result.claims.len(), claims.len());
        assert!(result.claims[0].known.is_none());
        assert_eq!(result.claims[1].correctness, Correctness::NotChecked);
        assert!(result.claims[1].witnesses.is_empty());
        assert!(result.summary.contains("dropped"));
        assert_eq!(result.concerns[0].status, ConcernStatus::NotCheckable);
        assert_eq!(result.concerns[1].status, ConcernStatus::Refuted);
        assert_eq!(result.concerns[2].status, ConcernStatus::NotCheckable);
        let oversized = audit(
            AuditOut {
                concerns: vec![
                    AuditedConcernOut {
                        concern: "A comparison".into(),
                        status: "confirmed".into(),
                        basis: "opened_source".into(),
                        note: "n".repeat(9_000),
                        evidence: format!(
                            "Work [opened: https://example.test/work]\n{}",
                            "e".repeat(2_000)
                        ),
                    },
                    AuditedConcernOut {
                        concern: "An argument".into(),
                        status: "confirmed".into(),
                        basis: "offline".into(),
                        note: "A checked case".into(),
                        evidence: "e".repeat(8_001),
                    },
                    AuditedConcernOut {
                        concern: "A comparison without an argument".into(),
                        status: "confirmed".into(),
                        basis: "opened_source".into(),
                        note: "A named work alone".into(),
                        evidence: "Work [opened: https://example.test/work]".into(),
                    },
                ],
                verdict: "accept".into(),
                summary: "Check bounded source evidence.".into(),
                ..Default::default()
            },
            &input,
            &claims,
        )
        .unwrap();
        assert_eq!(oversized.concerns.len(), 1);
        assert_eq!(oversized.concerns[0].status, ConcernStatus::NotCheckable);
        assert!(oversized.summary.contains("dropped"));
        let named = AuditOut {
            verdict: "reject".into(),
            summary: "Known main result.".into(),
            claims: vec![AuditedClaimOut {
                claim: "C1".into(),
                correctness: "correct".into(),
                comment: "An instantiation of the named work.".into(),
                shape: Some("bind_only".into()),
                known: Some(
                    "A named earlier work [opened: https://example.test/earlier-work]".into(),
                ),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(
            audit(named, &input, &claims).unwrap().claims[0]
                .known
                .as_deref(),
            Some("A named earlier work [opened: https://example.test/earlier-work]")
        );
        assert!(
            audit(
                AuditOut {
                    verdict: "made_up".into(),
                    ..Default::default()
                },
                &input,
                &claims
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod conjecture_tests {
    use super::*;
    use wishpool_core::model::{AuditedClaim, ConjectureStatus, RefereeAudit};
    use wishpool_review::{
        advisor::AdvisorInput,
        referee_prompts::{AuditOut, AuditedClaimOut, ConjectureOut, ReadingOut, RefereeOut},
    };

    fn reading() -> ConjectureOut {
        ConjectureOut {
            well_posed: Some(true),
            well_posed_reason: "Symbols and quantifiers are defined.".into(),
            status: "open".into(),
            status_reason: "No solution in the supplied material.".into(),
            escape: "content".into(),
            escape_reason: "Would require a new estimate.".into(),
            named_works: vec!["Named source [opened: https://example.test/named-source]".into()],
            ..Default::default()
        }
    }

    fn target(result: RefereeAudit) -> AuditedClaim {
        result
            .claims
            .into_iter()
            .find(|c| c.claim.as_str() == "C2")
            .unwrap()
    }

    #[test]
    fn conjecture_audit_requires_opened_source_identity_and_matching_evidence() {
        let claims = super::referee_tests::claims();
        let input = AdvisorInput {
            kind: "conjecture".into(),
            title: "Question".into(),
            abstract_text: String::new(),
            authors: vec![],
            statements: confirmed_statements(&claims),
            referee: RefereeOut::default(),
            text: Some("Named source states a particular case.".into()),
            source_dir: None,
            main_file: None,
            audit: None,
            decision: None,
        };
        let check = |conjecture: ConjectureOut| {
            audit(
                AuditOut {
                    verdict: "accept".into(),
                    summary: "Checked source.".into(),
                    claims: vec![AuditedClaimOut {
                        claim: "C2".into(),
                        comment: "A source-based reading.".into(),
                        conjecture: Some(conjecture),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                &input,
                &claims,
            )
            .unwrap()
        };
        let open = check(reading());
        assert!(target(open).conjecture.as_ref().unwrap().displayable());
        let mut external = reading();
        external.status = "known_true".into();
        external.status_basis = "external".into();
        external.status_evidence = "Referee attribution.".into();
        assert_eq!(
            target(check(external.clone()))
                .conjecture
                .as_ref()
                .unwrap()
                .status,
            ConjectureStatus::Unclear
        );
        external.status_basis = "offline".into();
        external.status_evidence = "The source's stated theorem specializes verbatim.".into();
        assert_eq!(
            target(check(external)).conjecture.as_ref().unwrap().status,
            ConjectureStatus::KnownTrue
        );
        let mut invented = reading();
        invented.named_works = vec!["Invented source".into()];
        let invalid = check(invented);
        assert!(target(invalid.clone()).conjecture.is_none());
        assert!(invalid.summary.contains("dropped"));
        for bad in ["made_up", "OPEN"] {
            let mut invalid = reading();
            invalid.status = bad.into();
            assert!(target(check(invalid)).conjecture.is_none());
        }
        let claim = AuditedClaimOut {
            claim: "C2".into(),
            comment: "Checked.".into(),
            conjecture: Some(reading()),
            ..Default::default()
        };
        let duplicate = audit(
            AuditOut {
                verdict: "accept".into(),
                summary: "Duplicate readings.".into(),
                claims: vec![claim.clone(), claim],
                ..Default::default()
            },
            &input,
            &claims,
        )
        .unwrap();
        assert!(target(duplicate).conjecture.is_none());
        let report = referee_for_kind(
            RefereeOut {
                claims: vec![ReadingOut {
                    claim: "C99".into(),
                    conjecture: Some(reading()),
                    ..Default::default()
                }],
                ..Default::default()
            },
            "Raw answer".into(),
            &claims,
            true,
        );
        assert!(report.claims.is_empty());
        assert!(!report.limits.is_empty());
    }
}
