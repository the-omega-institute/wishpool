use chrono::Utc;

use super::*;
use crate::{
    ids::ClaimId,
    model::{claim::tests::claim, *},
};

fn report(stage: Stage, outcome: Outcome, payload: StagePayload, human: bool) -> StageReport {
    StageReport {
        stage,
        outcome,
        summary: "s".into(),
        payload,
        evidence: vec![],
        reviewer: if human {
            ReviewerIdentity::Human {
                person: "ed".into(),
            }
        } else {
            ReviewerIdentity::Machine {
                account: "bot".into(),
                engine: "e".into(),
                model: None,
            }
        },
        claims_revision: 0,
        filed_at: Utc::now(),
    }
}

fn paper(claims: Vec<Claim>) -> Submission {
    let now = Utc::now();
    let mut s = Submission {
        id: "p".into(),
        submitter: "author".into(),
        title: "T".into(),
        abstract_text: String::new(),
        authors: vec![],
        ai_disclosure: AiDisclosure {
            level: AiUse::None,
            statement: "none".into(),
        },
        msc: vec![],
        doi: None,
        versions: vec![],
        extracted: claims.clone(),
        claims: vec![],
        claims_revision: 0,
        reports: vec![],
        status: SubmissionStatus::InReview,
        decision: None,
        open_to_contributors: false,
        analysis_visibility: Visibility::Undecided,
        formalization: FormalizationPlan::default(),
        conjectures: vec![],
        created_at: now,
        updated_at: now,
        revision: 0,
    };
    s.push_report(report(
        Stage::Hygiene,
        Outcome::Pass,
        StagePayload::Hygiene { checks: vec![] },
        false,
    ));
    s.push_report(report(
        Stage::Claims,
        Outcome::Pass,
        StagePayload::Claims { claims },
        true,
    ));
    s
}

fn arxiv(id: &str) -> Source {
    Source {
        kind: SourceKind::Arxiv,
        locator: id.into(),
        year: None,
    }
}

fn literature(prior: Vec<PriorWork>, human: bool) -> StageReport {
    let outcome = if human {
        Outcome::Pass
    } else {
        Outcome::NeedsHuman {
            question: "confirm".into(),
        }
    };
    report(
        Stage::Literature,
        outcome,
        StagePayload::Literature {
            prior,
            searched: vec!["OpenAlex".into()],
        },
        human,
    )
}

fn escape(assessments: Vec<(&str, ProofShape)>) -> StageReport {
    let assessments = assessments
        .into_iter()
        .map(|(id, shape)| EscapeAssessment {
            claim: id.into(),
            shape,
            witnesses: if shape == ProofShape::Content {
                vec!["the gap estimate".into()]
            } else {
                vec![]
            },
            rationale: "r".into(),
            escape_rate: None,
        })
        .collect();
    report(
        Stage::Escape,
        Outcome::Pass,
        StagePayload::Escape { assessments },
        true,
    )
}

fn main_and_lemma() -> Vec<Claim> {
    vec![
        claim("C1", ClaimKind::Theorem, ClaimRole::Main, &["C2"]),
        claim("C2", ClaimKind::Lemma, ClaimRole::Supporting, &[]),
    ]
}

#[test]
fn draft_waits_for_the_author() {
    let mut s = paper(main_and_lemma());
    s.status = SubmissionStatus::Draft;
    assert!(matches!(
        Policy::default().decide(&s, &[]),
        Decision::Pending {
            awaiting: Stage::Claims,
            ..
        }
    ));
    assert_eq!(
        Policy::default().next_machine_stage(&s),
        Some(Stage::Literature)
    );
}

#[test]
fn machine_reports_on_judgement_stages_are_proposals() {
    let mut s = paper(main_and_lemma());
    let policy = Policy::default();
    assert_eq!(policy.next_machine_stage(&s), Some(Stage::Literature));
    s.push_report(literature(vec![], false));
    assert!(matches!(
        policy.decide(&s, &[]),
        Decision::Pending {
            awaiting: Stage::Literature,
            ..
        }
    ));
    assert_eq!(policy.next_machine_stage(&s), None);
    s.push_report(literature(vec![], true));
    assert_eq!(policy.next_machine_stage(&s), Some(Stage::Escape));
    assert!(matches!(
        policy.decide(&s, &[]),
        Decision::Pending {
            awaiting: Stage::Escape,
            ..
        }
    ));
}

#[test]
fn a_new_main_result_with_content_is_accepted() {
    let mut s = paper(main_and_lemma());
    s.push_report(literature(vec![], true));
    s.push_report(escape(vec![
        ("C1", ProofShape::Content),
        ("C2", ProofShape::BindOnly),
    ]));
    assert_eq!(
        Policy::default().decide(&s, &[]),
        Decision::Accept {
            basis: AdmissionBasis::EscapeWitness
        }
    );
}

#[test]
fn content_only_in_a_lemma_does_not_pass() {
    let mut s = paper(main_and_lemma());
    s.push_report(literature(vec![], true));
    s.push_report(escape(vec![
        ("C1", ProofShape::BindOnly),
        ("C2", ProofShape::Content),
    ]));
    assert_eq!(
        Policy::default().decide(&s, &[]),
        Decision::NotAccepted {
            reasons: vec![RejectReason::BindOnly]
        }
    );
}

#[test]
fn a_known_main_result_does_not_pass() {
    let mut s = paper(main_and_lemma());
    let prior = PriorWork {
        claim: "C1".into(),
        source: arxiv("2401.00001"),
        relation: PriorRelation::Implies,
        note: String::new(),
    };
    s.push_report(literature(vec![prior], true));
    s.push_report(escape(vec![("C1", ProofShape::Content)]));
    let Decision::NotAccepted { reasons } = Policy::default().decide(&s, &[]) else {
        panic!("expected not accepted")
    };
    assert_eq!(
        reasons,
        vec![RejectReason::KnownResult {
            claim: ClaimId::from("C1"),
            prior: arxiv("2401.00001")
        }]
    );
}

#[test]
fn related_work_does_not_block() {
    let mut s = paper(main_and_lemma());
    let prior = PriorWork {
        claim: "C1".into(),
        source: arxiv("2401.00001"),
        relation: PriorRelation::Related,
        note: String::new(),
    };
    s.push_report(literature(vec![prior], true));
    s.push_report(escape(vec![("C1", ProofShape::Content)]));
    assert!(matches!(
        Policy::default().decide(&s, &[]),
        Decision::Accept { .. }
    ));
}

#[test]
fn every_main_result_needs_an_assessment() {
    let mut claims = main_and_lemma();
    claims.push(claim("C3", ClaimKind::Theorem, ClaimRole::Main, &[]));
    let mut s = paper(claims);
    s.push_report(literature(vec![], true));
    s.push_report(escape(vec![("C1", ProofShape::Content)]));
    assert!(matches!(
        Policy::default().decide(&s, &[]),
        Decision::Pending {
            awaiting: Stage::Escape,
            ..
        }
    ));
}

#[test]
fn settling_a_named_problem_passes() {
    let mut main = claim("C1", ClaimKind::Theorem, ClaimRole::Main, &[]);
    main.settles = Some(OpenProblemRef {
        name: "Question 4.2 of X".into(),
        source: arxiv("2301.00002"),
    });
    let mut s = paper(vec![main]);
    s.push_report(literature(vec![], true));
    s.push_report(escape(vec![("C1", ProofShape::BindOnly)]));
    assert_eq!(
        Policy::default().decide(&s, &[]),
        Decision::Accept {
            basis: AdmissionBasis::OpenProblemSettlement
        }
    );
}

#[test]
fn hygiene_failure_rejects_and_conjectures_are_not_main_results() {
    let mut s = paper(vec![claim(
        "C1",
        ClaimKind::Conjecture,
        ClaimRole::Main,
        &[],
    )]);
    s.push_report(literature(vec![], true));
    s.push_report(escape(vec![]));
    assert_eq!(
        Policy::default().decide(&s, &[]),
        Decision::NotAccepted {
            reasons: vec![RejectReason::NoMainResult]
        }
    );
    let fail = Outcome::Fail {
        reason: RejectReason::Hygiene {
            detail: "PDF".into(),
        },
    };
    s.push_report(report(
        Stage::Hygiene,
        fail,
        StagePayload::Hygiene { checks: vec![] },
        false,
    ));
    assert!(
        matches!(Policy::default().decide(&s, &[]), Decision::NotAccepted { reasons } if matches!(reasons[0], RejectReason::Hygiene { .. }))
    );
}

#[test]
fn a_changed_claim_set_voids_literature_and_escape() {
    let mut s = paper(main_and_lemma());
    s.push_report(literature(vec![], true));
    s.push_report(escape(vec![("C1", ProofShape::Content)]));
    s.push_report(report(
        Stage::Claims,
        Outcome::Pass,
        StagePayload::Claims {
            claims: vec![claim("C1", ClaimKind::Theorem, ClaimRole::Main, &[])],
        },
        true,
    ));
    assert!(s.latest_report(Stage::Literature).is_none());
    assert!(matches!(
        Policy::default().decide(&s, &[]),
        Decision::Pending {
            awaiting: Stage::Literature,
            ..
        }
    ));
}
