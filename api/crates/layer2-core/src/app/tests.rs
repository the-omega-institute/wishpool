use std::{collections::BTreeSet, sync::Arc};

use chrono::Datelike;

use super::*;
use crate::{
    CoreError,
    ids::{ClaimId, PersonId, RecordId},
    judgement::Standing,
    memory::MemoryStores,
    model::*,
    policy::{AdmissionBasis, Decision},
    ports::{
        ContributionFilter, JobKind, PaperReader, ReadPaper, ReadStatement, SystemClock, TaskFilter,
    },
};

/// Reads every upload as the same paper, with the statements named in the
/// filename: `t` theorem, `l` lemma, `c` conjecture (`tlc.tex`).
struct FakeReader;

impl PaperReader for FakeReader {
    fn read(&self, _bytes: &[u8], filename: &str) -> Result<ReadPaper, String> {
        let stem = filename.split('.').next().unwrap_or_default();
        let statements = stem
            .chars()
            .map(|c| {
                let (kind, name) = match c {
                    't' => (ClaimKind::Theorem, "Theorem"),
                    'l' => (ClaimKind::Lemma, "Lemma"),
                    'c' => (ClaimKind::Conjecture, "Conjecture"),
                    _ => return Err(format!("unknown statement letter {c}")),
                };
                Ok(ReadStatement {
                    kind,
                    display_name: name.into(),
                    title: None,
                    latex_label: None,
                    body: format!("A {name} body"),
                    has_proof: kind != ClaimKind::Conjecture,
                    section: Some("1".into()),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ReadPaper {
            main_file: "main.tex".into(),
            title: Some("Gaps in zero runs".into()),
            authors: vec!["A. Author".into()],
            abstract_text: Some("We bound the gaps.".into()),
            statements,
            macros: Default::default(),
            warnings: vec![],
        })
    }
}

struct World {
    app: Arc<App>,
    stores: Arc<MemoryStores>,
}

impl World {
    async fn new() -> Self {
        let stores = Arc::new(MemoryStores::default());
        let app = App::new(
            stores.ports(Arc::new(SystemClock), Arc::new(FakeReader)),
            Policy::default(),
            BTreeSet::from([PersonId::from("admin")]),
        );
        Self { app, stores }
    }

    async fn person(&self, sub: &str, roles: &[Role]) -> Caller {
        let identity = |s: &str| VerifiedIdentity {
            subject: s.into(),
            name: Some(s.into()),
            email: None,
            picture: None,
        };
        self.app.sign_in(&identity(sub)).await.unwrap();
        if !roles.is_empty() {
            let admin = self.app.caller(&identity("admin")).await.unwrap();
            self.app
                .set_roles(&admin, &sub.into(), roles)
                .await
                .unwrap();
        }
        self.app.caller(&identity(sub)).await.unwrap()
    }

    async fn submit(&self, author: &Caller, statements: &str, open: bool) -> Submission {
        self.submit_with_doi(author, statements, open, None).await
    }

    async fn submit_with_doi(
        &self,
        author: &Caller,
        statements: &str,
        open: bool,
        doi: Option<&str>,
    ) -> Submission {
        let new = NewPaper {
            kind: crate::model::SubmissionKind::Paper,
            make_public_after_acceptance: true,
            typed_conjecture: None,
            ai_disclosure: AiDisclosure {
                level: AiUse::Assisted,
                statement: "A model checked Lemma 2.".into(),
            },
            authors: vec![],
            msc: vec!["11B83".into()],
            doi: doi.map(str::to_owned),
            open_to_contributors: open,
        };
        let upload = Upload {
            filename: format!("{statements}.tex"),
            bytes: b"\\documentclass{article}".to_vec(),
        };
        self.app.submit_paper(author, new, upload).await.unwrap()
    }

    /// Compile (as the worker would) and confirm the statements: theorems
    /// are main results.
    async fn review_paper(&self, author: &Caller, paper: &Submission) -> Submission {
        self.review_version(author, paper, 1).await
    }

    async fn review_version(
        &self,
        author: &Caller,
        paper: &Submission,
        version: u32,
    ) -> Submission {
        let bot = self.person("bot", &[Role::Reviewer]).await;
        self.app
            .record_compilation(&bot, &paper.id, version, Ok(b"%PDF".to_vec()))
            .await
            .unwrap();
        let current = self.app.submission(author, &paper.id).await.unwrap();
        let confirmations = current
            .extracted
            .iter()
            .map(|c| ClaimConfirmation {
                depends_on_conjectures: vec![],
                id: c.id.clone(),
                kind: c.kind,
                role: c.role,
                depends_on: vec![],
                settles: None,
                excluded: false,
            })
            .collect();
        self.app
            .confirm_claims(author, &paper.id, confirmations)
            .await
            .unwrap()
    }

    async fn literature(
        &self,
        editor: &Caller,
        paper: &Submission,
        prior: Vec<PriorWork>,
    ) -> Submission {
        let draft = ReportDraft {
            outcome: Outcome::Pass,
            summary: "Searched opened literature sources.".into(),
            payload: StagePayload::Literature {
                prior,
                searched: vec!["Opened source: zero runs gaps".into()],
            },
            evidence: vec![],
        };
        self.app
            .file_report(editor, &paper.id, Stage::Literature, draft, None)
            .await
            .unwrap()
    }
}

fn content() -> (ProofShape, Vec<String>) {
    (ProofShape::Content, vec!["the run-length gap bound".into()])
}

fn agent(model: &str) -> AgentInfo {
    AgentInfo {
        tool: "claude-code".into(),
        model: model.into(),
    }
}

#[tokio::test]
async fn managed_agent_steps_keep_identity_and_allow_only_existing_cleanup_after_withdrawal() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("reviewer", &[Role::Reviewer]).await;
    let paper = w.submit(&author, "t", false).await;
    let paper = w.review_paper(&author, &paper).await;
    let mut step = AgentStep {
        version: 1,
        claims_revision: paper.claims_revision,
        progress: serde_json::json!({"agent_id":"agt_test"}),
    };
    assert!(
        w.app
            .save_agent_step(&author, &paper.id, "step", step.clone())
            .await
            .is_err()
    );
    w.app
        .save_agent_step(&reviewer, &paper.id, "step", step.clone())
        .await
        .unwrap();
    let changed = AgentStep {
        version: 2,
        ..step.clone()
    };
    assert!(matches!(
        w.app
            .save_agent_step(&reviewer, &paper.id, "step", changed)
            .await,
        Err(CoreError::Conflict(_))
    ));
    w.app.withdraw(&author, &paper.id).await.unwrap();
    step.progress["deleted"] = serde_json::json!(true);
    w.app
        .save_agent_step(&reviewer, &paper.id, "step", step.clone())
        .await
        .unwrap();
    assert!(matches!(
        w.app
            .save_agent_step(&reviewer, &paper.id, "new-step", step)
            .await,
        Err(CoreError::Conflict(_))
    ));
    assert_eq!(
        w.app
            .agent_step(&reviewer, &paper.id, "step")
            .await
            .unwrap()
            .unwrap()
            .progress["deleted"],
        true
    );
    assert!(w.app.agent_steps(&author, &paper.id).await.is_err());
    assert!(
        !serde_json::to_string(&w.app.referee(&author, &paper.id).await.unwrap())
            .unwrap()
            .contains("agt_test")
    );
}

#[tokio::test]
async fn upload_confirm_review_accept_and_formalize() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let editor = w.person("editor", &[Role::Editor]).await;
    let reader = w.person("reader", &[]).await;

    let paper = w
        .submit_with_doi(&author, "tlc", false, Some(" https://doi.org/10.1000/xyz "))
        .await;
    assert_eq!(paper.status, SubmissionStatus::Draft);
    assert_eq!(paper.doi.as_deref(), Some("10.1000/xyz"));
    assert_eq!(
        w.app.submission(&author, &paper.id).await.unwrap().doi,
        paper.doi
    );
    assert_eq!(paper.title, "Gaps in zero runs");
    assert_eq!(paper.extracted.len(), 3);
    assert_eq!(paper.extracted[0].role, ClaimRole::Main);
    assert_eq!(paper.extracted[1].role, ClaimRole::Supporting);
    assert!(
        w.stores
            .queued()
            .await
            .contains(&(paper.id.clone(), JobKind::Compile))
    );
    // Nobody else sees a draft.
    assert!(matches!(
        w.app.submission(&reader, &paper.id).await,
        Err(CoreError::NotFound { .. })
    ));

    let paper = w.review_paper(&author, &paper).await;
    assert_eq!(paper.status, SubmissionStatus::InReview);
    assert!(
        w.stores
            .queued()
            .await
            .contains(&(paper.id.clone(), JobKind::Stage(Stage::Literature)))
    );

    // Escape cannot be reported before the literature passes.
    let early = ReportDraft {
        outcome: Outcome::Pass,
        summary: "x".into(),
        payload: StagePayload::Escape {
            assessments: vec![],
        },
        evidence: vec![],
    };
    assert!(matches!(
        w.app
            .file_report(&editor, &paper.id, Stage::Escape, early, None)
            .await,
        Err(CoreError::Conflict(_))
    ));
    w.literature(&editor, &paper, vec![]).await;

    // Without a judgement for the main result nothing can be adopted.
    assert!(w.app.adopt_judgements(&editor, &paper.id).await.is_err());
    // Usage sent by a client may omit `metered`.
    let usage: TokenUsage = serde_json::from_str(r#"{"input": 5, "output": 1}"#).unwrap();
    assert!(!usage.metered);
    let (shape, witnesses) = content();
    w.app
        .judge_claim(
            &editor,
            &paper.id,
            &"C1".into(),
            shape,
            witnesses,
            "New estimate.".into(),
        )
        .await
        .unwrap();
    let paper = w.app.adopt_judgements(&editor, &paper.id).await.unwrap();
    assert_eq!(
        paper.decision,
        Some(Decision::Accept {
            basis: AdmissionBasis::EscapeWitness
        })
    );

    let paper = w.app.decide(&editor, &paper.id).await.unwrap();
    let year = chrono::Utc::now().year();
    assert_eq!(
        paper.status,
        SubmissionStatus::Accepted {
            record: RecordId::format(year, 1)
        }
    );
    assert_eq!(paper.conjectures.len(), 1);
    assert_eq!(paper.conjectures[0].state, ConjectureState::Screening);

    // Public mathematical inputs appear at acceptance; review stays private.
    let record = RecordId::format(year, 1);
    let public = w.app.paper(&record).await.unwrap();
    assert_eq!(public.summary.doi.as_deref(), Some("10.1000/xyz"));
    assert_eq!(public.claims.len(), 3);
    assert!(public.new_content.is_empty());
    assert!(
        serde_json::to_value(&public)
            .unwrap()
            .get("analysis")
            .is_none()
    );
    assert_eq!(w.app.list_papers(None, None).await.unwrap().items.len(), 1);
    assert!(w.app.paper_file(None, &paper.id, None, true).await.is_ok());
    assert!(
        w.app
            .paper_file(None, &paper.id, None, false)
            .await
            .is_err(),
        "sources stay private"
    );
    assert!(
        w.app
            .set_analysis_visibility(&reader, &paper.id, Visibility::Public)
            .await
            .is_err()
    );
    w.app
        .set_analysis_visibility(&author, &paper.id, Visibility::Public)
        .await
        .unwrap();
    let analysis = w.app.analysis(&author, &paper.id).await.unwrap();
    assert_eq!(
        (
            analysis.main_results,
            analysis.main_with_content,
            analysis.main_known
        ),
        (1, 1, 0)
    );
    assert_eq!(
        analysis.claims[0].judgement.as_ref().unwrap().standing,
        Standing::Confirmed
    );

    // Formalization: the editor proposes, the author approves, the editor
    // records the checked proof.
    let c1 = ClaimId::from("C1");
    assert!(
        w.app
            .propose_formalization(&editor, &paper.id, &"C3".into(), "open".into())
            .await
            .is_err()
    );
    w.app
        .propose_formalization(&editor, &paper.id, &c1, "The headline bound.".into())
        .await
        .unwrap();
    let artifact = FormalArtifact {
        repository: "https://github.com/omega/gaps-lean".into(),
        commit: "b".repeat(40),
        declarations: vec!["Gaps.main".into()],
    };
    assert!(
        w.app
            .verify_formalization(&editor, &paper.id, &c1, artifact.clone(), vec![], None)
            .await
            .is_err(),
        "the author approves first"
    );
    assert!(
        w.app
            .respond_to_formalization(&editor, &paper.id, &c1, true, String::new())
            .await
            .is_err()
    );
    w.app
        .respond_to_formalization(&author, &paper.id, &c1, true, String::new())
        .await
        .unwrap();
    assert!(
        w.app
            .verify_formalization(
                &editor,
                &paper.id,
                &c1,
                artifact.clone(),
                vec!["sorryAx".into()],
                None
            )
            .await
            .is_err()
    );
    w.app
        .verify_formalization(
            &editor,
            &paper.id,
            &c1,
            artifact.clone(),
            vec!["propext".into()],
            None,
        )
        .await
        .unwrap();
    let public = w.app.paper(&record).await.unwrap();
    assert_eq!(public.claims[0].lean, Some(artifact));
    assert_eq!(public.summary.lean_verified, 1);

    // The conjecture's follow-up is public with the analysis.
    w.app
        .update_conjecture(
            &editor,
            &paper.id,
            &"C3".into(),
            ConjectureState::NotPursued {
                reason: "Out of reach for now.".into(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        w.app
            .submission(&author, &paper.id)
            .await
            .unwrap()
            .conjectures[0]
            .state,
        ConjectureState::NotPursued { .. }
    ));
    assert!(
        w.app
            .update_conjecture(&editor, &paper.id, &c1, ConjectureState::TakenUp)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn known_results_are_not_accepted_and_may_be_revised() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let editor = w.person("editor", &[Role::Editor]).await;
    let paper = w
        .review_paper(&author, &w.submit(&author, "tl", false).await)
        .await;
    let prior = PriorWork {
        claim: "C1".into(),
        source: Source {
            kind: SourceKind::Arxiv,
            locator: "2401.00001".into(),
            year: Some(2024),
        },
        relation: PriorRelation::Same,
        note: "Theorem 2 there.".into(),
    };
    w.literature(&editor, &paper, vec![prior]).await;
    let (shape, witnesses) = content();
    w.app
        .judge_claim(
            &editor,
            &paper.id,
            &"C1".into(),
            shape,
            witnesses,
            "r".into(),
        )
        .await
        .unwrap();
    w.app.adopt_judgements(&editor, &paper.id).await.unwrap();
    let paper = w.app.decide(&editor, &paper.id).await.unwrap();
    assert_eq!(paper.status, SubmissionStatus::NotAccepted);
    assert!(
        matches!(&paper.decision, Some(Decision::NotAccepted { reasons }) if matches!(reasons[0], RejectReason::KnownResult { .. }))
    );
    // Nothing public; the author keeps the report and may revise.
    assert!(
        w.app
            .list_papers(None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(w.app.paper_file(None, &paper.id, None, true).await.is_err());
    assert!(w.app.analysis(&author, &paper.id).await.unwrap().main_known == 1);
    let upload = Upload {
        filename: "tt.tex".into(),
        bytes: b"v2".to_vec(),
    };
    let revised = w
        .app
        .upload_version(&author, &paper.id, upload, "New second theorem.".into())
        .await
        .unwrap();
    assert_eq!(
        (
            revised.status.clone(),
            revised.versions.len(),
            revised.extracted.len()
        ),
        (SubmissionStatus::Draft, 2, 2)
    );
}

#[tokio::test]
async fn authority_and_limits() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let other = w.person("other", &[]).await;
    let editor_author = w.person("ea", &[Role::Editor]).await;
    let paper = w.submit(&author, "t", false).await;
    let confirm = vec![ClaimConfirmation {
        depends_on_conjectures: vec![],
        id: "C1".into(),
        kind: ClaimKind::Theorem,
        role: ClaimRole::Main,
        depends_on: vec![],
        settles: None,
        excluded: false,
    }];
    assert!(
        matches!(
            w.app
                .confirm_claims(&other, &paper.id, confirm.clone())
                .await,
            Err(CoreError::NotFound { .. })
        ),
        "strangers do not learn the paper exists"
    );
    // At least one proved main result.
    let no_main = vec![ClaimConfirmation {
        depends_on_conjectures: vec![],
        role: ClaimRole::Supporting,
        ..confirm[0].clone()
    }];
    assert!(
        w.app
            .confirm_claims(&author, &paper.id, no_main)
            .await
            .is_err()
    );
    let excluded = vec![ClaimConfirmation {
        depends_on_conjectures: vec![],
        excluded: true,
        ..confirm[0].clone()
    }];
    assert!(
        w.app
            .confirm_claims(&author, &paper.id, excluded)
            .await
            .is_err()
    );

    // An editor cannot review their own paper.
    let own = w
        .review_paper(&editor_author, &w.submit(&editor_author, "t", false).await)
        .await;
    let draft = ReportDraft {
        outcome: Outcome::Pass,
        summary: "x".into(),
        payload: StagePayload::Literature {
            prior: vec![],
            searched: vec!["x".into()],
        },
        evidence: vec![],
    };
    assert!(matches!(
        w.app
            .file_report(&editor_author, &own.id, Stage::Literature, draft, None)
            .await,
        Err(CoreError::Forbidden(_))
    ));
    // Statements are the author's: nobody files S1.
    let claims = ReportDraft {
        outcome: Outcome::Pass,
        summary: "x".into(),
        payload: StagePayload::Claims { claims: vec![] },
        evidence: vec![],
    };
    assert!(
        w.app
            .file_report(&editor_author, &paper.id, Stage::Claims, claims, None)
            .await
            .is_err()
    );

    // At most three papers in draft or review.
    w.submit(&author, "t", false).await;
    w.submit(&author, "t", false).await;
    let new = NewPaper {
        kind: crate::model::SubmissionKind::Paper,
        make_public_after_acceptance: true,
        typed_conjecture: None,
        ai_disclosure: AiDisclosure {
            level: AiUse::None,
            statement: "None.".into(),
        },
        authors: vec![],
        msc: vec![],
        doi: None,
        open_to_contributors: false,
    };
    let fourth = w
        .app
        .submit_paper(
            &author,
            new,
            Upload {
                filename: "t.tex".into(),
                bytes: b"x".to_vec(),
            },
        )
        .await;
    assert!(matches!(fourth, Err(CoreError::Conflict(_))));
    w.app.withdraw(&author, &paper.id).await.unwrap();
    assert_eq!(
        w.app
            .list_submissions(&author, SubmissionScope::Mine, None, None)
            .await
            .unwrap()
            .items
            .len(),
        3
    );
    assert!(
        w.app
            .list_submissions(&author, SubmissionScope::Queue, None, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn contributors_judge_by_quorum_only_on_opted_in_papers() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let editor = w.person("editor", &[Role::Editor]).await;
    let alice = w.person("alice", &[]).await;
    let bob = w.person("bob", &[]).await;

    let closed = w
        .review_paper(&author, &w.submit(&author, "tl", false).await)
        .await;
    assert!(
        w.app
            .generate_review_tasks(&editor, &closed.id)
            .await
            .is_err()
    );
    let all = w
        .app
        .list_tasks(TaskFilter::default(), None, None)
        .await
        .unwrap();
    assert!(all.items.is_empty());

    let paper = w
        .review_paper(&author, &w.submit(&author, "tlc", true).await)
        .await;
    let tasks = w
        .app
        .list_tasks(
            TaskFilter {
                submission: Some(paper.id.clone()),
                ..TaskFilter::default()
            },
            None,
            None,
        )
        .await
        .unwrap()
        .items;
    // A judgement per proved statement, a literature check per main result.
    assert_eq!(tasks.len(), 3);
    let judge = tasks
        .iter()
        .find(|t| t.kind == TaskKind::JudgeEscape && t.target.claim == ClaimId::from("C1"))
        .unwrap();
    assert!(matches!(
        w.app
            .lease_task(&author, &judge.id, ContributionMode::OwnAgent)
            .await,
        Err(CoreError::Forbidden(_))
    ));
    let context = w.app.task_context(&alice, &judge.id).await.unwrap();
    assert_eq!(context.claim.kind, ClaimKind::Theorem);

    let (shape, witnesses) = content();
    let judgement = |model: &str| NewContribution {
        agent: agent(model),
        output: ContributionOutput::Judgement {
            shape,
            witnesses: witnesses.clone(),
            rationale: "A new estimate.".into(),
        },
        tokens: Some(TokenUsage {
            input: 1000,
            output: 200,
            metered: true,
        }),
    };
    w.app
        .lease_task(&alice, &judge.id, ContributionMode::OwnAgent)
        .await
        .unwrap();
    let first = w
        .app
        .submit_contribution(&alice, &judge.id, judgement("claude-opus-5-5"))
        .await
        .unwrap();
    assert!(
        !first.tokens.unwrap().metered,
        "usage sent through the API is self-reported"
    );
    assert!(
        w.app
            .lease_task(&alice, &judge.id, ContributionMode::OwnAgent)
            .await
            .is_err(),
        "independent contributors are needed"
    );
    w.app
        .lease_task(&bob, &judge.id, ContributionMode::OwnAgent)
        .await
        .unwrap();
    w.app
        .submit_contribution(&bob, &judge.id, judgement("gpt-5.5"))
        .await
        .unwrap();

    let task = w.app.task(&judge.id).await.unwrap();
    assert!(matches!(task.status, TaskStatus::Done { .. }));
    let verified = w
        .app
        .list_contributions(
            ContributionFilter {
                status: Some("verified".into()),
                ..Default::default()
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(verified.items.len(), 2);
    let analysis = w.app.analysis(&editor, &paper.id).await.unwrap();
    assert_eq!(
        analysis.claims[0].judgement.as_ref().unwrap().standing,
        Standing::Corroborated
    );
    let credits = w.app.credits(None).await.unwrap();
    assert_eq!(
        credits
            .iter()
            .map(|c| (c.verified, c.reported_tokens))
            .collect::<Vec<_>>(),
        vec![(1, 1200), (1, 1200)]
    );

    // Closing the paper to contributors closes its open tasks and hides them.
    w.app
        .set_open_to_contributors(&author, &paper.id, false)
        .await
        .unwrap();
    let lit = tasks
        .iter()
        .find(|t| t.kind == TaskKind::LiteratureCheck)
        .unwrap();
    assert!(matches!(
        w.app.task(&lit.id).await.unwrap().status,
        TaskStatus::Closed { .. }
    ));
    assert!(w.app.task_context(&alice, &lit.id).await.is_err());
}

#[tokio::test]
async fn a_changed_claim_set_voids_judgements_and_tasks() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let editor = w.person("editor", &[Role::Editor]).await;
    let paper = w
        .review_paper(&author, &w.submit(&author, "tl", true).await)
        .await;
    let (shape, witnesses) = content();
    w.app
        .judge_claim(
            &editor,
            &paper.id,
            &"C1".into(),
            shape,
            witnesses,
            "r".into(),
        )
        .await
        .unwrap();
    assert!(
        w.app.analysis(&author, &paper.id).await.unwrap().claims[0]
            .judgement
            .is_some()
    );

    let upload = Upload {
        filename: "tlt.tex".into(),
        bytes: b"v2".to_vec(),
    };
    let revised = w
        .app
        .upload_version(&author, &paper.id, upload, String::new())
        .await
        .unwrap();
    let revised = w.review_version(&author, &revised, 2).await;
    assert!(
        w.app
            .analysis(&author, &revised.id)
            .await
            .unwrap()
            .claims
            .iter()
            .all(|c| c.judgement.is_none())
    );
    let tasks = w
        .app
        .list_tasks(
            TaskFilter {
                submission: Some(paper.id.clone()),
                ..TaskFilter::default()
            },
            Some(100),
            None,
        )
        .await
        .unwrap()
        .items;
    let open = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Open)
        .count();
    let closed = tasks
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::Closed { .. }))
        .count();
    // Old set: C1 judge (settled by the editor), C1 literature, C2 judge.
    // New set: three proved statements, two of them main results.
    assert_eq!((open, closed), (5, 3));
}

#[tokio::test]
async fn formalization_tasks_credit_the_contributor() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let editor = w.person("editor", &[Role::Editor]).await;
    let carol = w.person("carol", &[]).await;
    let paper = w
        .review_paper(&author, &w.submit(&author, "t", true).await)
        .await;
    w.literature(&editor, &paper, vec![]).await;
    let (shape, witnesses) = content();
    let c1 = ClaimId::from("C1");
    w.app
        .judge_claim(&editor, &paper.id, &c1, shape, witnesses, "r".into())
        .await
        .unwrap();
    w.app.adopt_judgements(&editor, &paper.id).await.unwrap();
    w.app.decide(&editor, &paper.id).await.unwrap();

    w.app
        .propose_formalization(&editor, &paper.id, &c1, "Headline.".into())
        .await
        .unwrap();
    w.app
        .respond_to_formalization(&author, &paper.id, &c1, true, String::new())
        .await
        .unwrap();
    let filter = TaskFilter {
        kind: Some(TaskKind::Formalize),
        ..TaskFilter::default()
    };
    let task = w
        .app
        .list_tasks(filter, None, None)
        .await
        .unwrap()
        .items
        .remove(0);
    assert!(
        w.app
            .lease_task(&carol, &task.id, ContributionMode::OwnAgent)
            .await
            .is_err(),
        "no repository yet"
    );
    w.app
        .set_formalization_repository(
            &editor,
            &paper.id,
            "https://github.com/omega/gaps-lean".into(),
        )
        .await
        .unwrap();
    assert!(
        w.app
            .lease_task(&carol, &task.id, ContributionMode::Hosted)
            .await
            .is_err(),
        "formalization needs a full agent"
    );
    w.app
        .lease_task(&carol, &task.id, ContributionMode::OwnAgent)
        .await
        .unwrap();
    let elsewhere = NewContribution {
        agent: agent("claude-opus-5-5"),
        output: ContributionOutput::PullRequest {
            url: "https://github.com/x/y/pull/1".into(),
        },
        tokens: None,
    };
    assert!(
        w.app
            .submit_contribution(&carol, &task.id, elsewhere)
            .await
            .is_err()
    );
    let pr = NewContribution {
        agent: agent("claude-opus-5-5"),
        output: ContributionOutput::PullRequest {
            url: "https://github.com/omega/gaps-lean/pull/7".into(),
        },
        tokens: None,
    };
    let contribution = w
        .app
        .submit_contribution(&carol, &task.id, pr)
        .await
        .unwrap();

    let artifact = FormalArtifact {
        repository: "https://github.com/omega/gaps-lean".into(),
        commit: "c".repeat(40),
        declarations: vec!["Gaps.main".into()],
    };
    w.app
        .verify_formalization(
            &editor,
            &paper.id,
            &c1,
            artifact,
            vec![],
            Some(contribution.id.clone()),
        )
        .await
        .unwrap();
    let credits = w.app.credits(Some(&carol.person)).await.unwrap();
    assert_eq!(
        (credits[0].verified, credits[0].verified_formalizations),
        (1, 1)
    );
    assert!(matches!(
        w.app.task(&task.id).await.unwrap().status,
        TaskStatus::Done { .. }
    ));
}

fn done_step<T>(result: T) -> Step<T> {
    let mut step = Step::pending();
    step.attempts = 1;
    step.state = StepState::Done {
        result,
        at: chrono::Utc::now(),
    };
    step
}

fn referee_report(recommendation: Option<Recommendation>) -> RefereeReport {
    RefereeReport {
        recommendation,
        summary: "A review".into(),
        strengths: vec![],
        concerns: vec![],
        claims: vec![],
        limits: vec![],
        text: "full answer".into(),
    }
}

#[tokio::test]
async fn confirm_enqueues_referee_and_begin_is_idempotent() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("referee", &[Role::Reviewer]).await;
    let paper = w.submit(&author, "t", false).await;
    let paper = w.review_paper(&author, &paper).await;
    let mut kinds = vec![];
    while let Some(job) = w.stores.take_job().await {
        kinds.push(job.kind);
    }
    assert!(kinds.contains(&JobKind::Referee));
    let first = w
        .app
        .begin_referee_round(&reviewer, &paper.id)
        .await
        .unwrap();
    let second = w
        .app
        .begin_referee_round(&reviewer, &paper.id)
        .await
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.rounds.len(), 1);
    let mut changed = paper.clone();
    changed.claims_revision += 1;
    w.app.save(&mut changed).await.unwrap();
    assert_eq!(
        w.app
            .begin_referee_round(&reviewer, &paper.id)
            .await
            .unwrap()
            .rounds
            .len(),
        2
    );
}

fn audit_for(
    paper: &Submission,
    verdict: Recommendation,
    correctness: Correctness,
    shape: ProofShape,
    known: Option<String>,
) -> RefereeAudit {
    RefereeAudit {
        verdict,
        agrees_with_referee: false,
        summary: "The main argument was checked against the source.".into(),
        claims: paper
            .claims
            .iter()
            .map(|c| AuditedClaim {
                conjecture: None,
                claim: c.id.clone(),
                correctness,
                comment: "The proof uses a new intermediate estimate.".into(),
                shape: Some(shape),
                witnesses: if shape == ProofShape::Content {
                    vec!["The gap estimate".into()]
                } else {
                    vec![]
                },
                known: known.clone(),
                referee_agreed: false,
            })
            .collect(),
        concerns: vec![],
    }
}

async fn audited(
    w: &World,
    reviewer: &Caller,
    paper: &Submission,
    audit: RefereeAudit,
) -> Submission {
    w.app
        .update_referee_round(reviewer, &paper.id, 1, RoundUpdate::Audit(done_step(audit)))
        .await
        .unwrap();
    w.app
        .apply_referee_audit(reviewer, &paper.id, 1)
        .await
        .unwrap()
}

#[tokio::test]
async fn audit_orders_and_fences_decision_advice_letter_then_formal() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let auditor = w.person("wishpool:auditor", &[Role::Reviewer]).await;
    let paper = w.submit(&author, "tl", false).await;
    let paper = w.review_paper(&author, &paper).await;
    w.app
        .begin_referee_round(&auditor, &paper.id)
        .await
        .unwrap();
    let audit = done_step(audit_for(
        &paper,
        Recommendation::MajorRevision,
        Correctness::Correct,
        ProofShape::Content,
        None,
    ));
    assert!(
        w.app
            .update_referee_round(&auditor, &paper.id, 1, RoundUpdate::Audit(audit.clone()))
            .await
            .is_err()
    );
    let advice = done_step(Advice {
        summary: "help".into(),
        improvements: vec![],
        formalization: vec![FormalizationCandidate {
            claim: paper.claims[0].id.clone(),
            feasibility: Feasibility::Ready,
            mathlib: vec![],
            missing: vec![],
            lean_sketch: String::new(),
            plan: "state and prove".into(),
            effort: Effort::Small,
        }],
    });
    let letter = done_step(LetterDraft {
        subject: "Review".into(),
        body: "Accepted with its record. Useful feedback.".into(),
        note: "Argument".into(),
    });
    let formal = done_step(FormalProbe {
        toolchain: "lean".into(),
        attempts: vec![FormalAttempt {
            claim: paper.claims[0].id.clone(),
            outcome: ProbeOutcome::Compiled,
            theorem: Some("Wishpool.C1.main".into()),
            lean: "private source".into(),
            axioms: vec![],
            note: "private note".into(),
            log: "private log".into(),
        }],
        summary: "private summary".into(),
    });
    let report = done_step(referee_report(Some(Recommendation::Reject)));
    w.app
        .update_referee_round(&auditor, &paper.id, 1, RoundUpdate::Referee(report.clone()))
        .await
        .unwrap();
    for update in [
        RoundUpdate::Advice(advice.clone()),
        RoundUpdate::Letter(letter.clone()),
        RoundUpdate::Formal(formal.clone()),
    ] {
        assert!(
            w.app
                .update_referee_round(&auditor, &paper.id, 1, update)
                .await
                .is_err()
        );
    }
    w.app
        .update_referee_round(&auditor, &paper.id, 1, RoundUpdate::Audit(audit.clone()))
        .await
        .unwrap();
    assert!(
        w.app
            .update_referee_round(&auditor, &paper.id, 1, RoundUpdate::Advice(advice.clone()))
            .await
            .is_err()
    );
    let decided = w
        .app
        .apply_referee_audit(&auditor, &paper.id, 1)
        .await
        .unwrap();
    assert!(matches!(decided.status, SubmissionStatus::Accepted { .. }));
    w.app
        .update_referee_round(&auditor, &paper.id, 1, RoundUpdate::Advice(advice.clone()))
        .await
        .unwrap();
    assert!(
        w.app
            .update_referee_round(&auditor, &paper.id, 1, RoundUpdate::Formal(formal.clone()))
            .await
            .is_err()
    );
    let file = w
        .app
        .update_referee_round(&auditor, &paper.id, 1, RoundUpdate::Letter(letter.clone()))
        .await
        .unwrap();
    assert_eq!(file.letters.len(), 1);
    assert_eq!(
        file.letters[0].assessment,
        Some(Recommendation::MajorRevision)
    );
    assert_eq!(file.letters[0].sent_by, auditor.person);
    assert!(!file.letters[0].edited);
    w.app
        .update_referee_round(&auditor, &paper.id, 1, RoundUpdate::Formal(formal.clone()))
        .await
        .unwrap();
    for update in [
        RoundUpdate::Referee(report),
        RoundUpdate::Audit(audit),
        RoundUpdate::Advice(advice),
        RoundUpdate::Letter(letter),
        RoundUpdate::Formal(formal),
    ] {
        assert!(
            w.app
                .update_referee_round(&auditor, &paper.id, 1, update)
                .await
                .is_err()
        );
    }
    let coauthor = w.person("linked-coauthor", &[]).await;
    let stranger = w.person("stranger", &[]).await;
    let mut linked = w.app.submission(&author, &paper.id).await.unwrap();
    linked.authors[0].person = Some(coauthor.person.clone());
    w.app.save(&mut linked).await.unwrap();
    assert_eq!(
        w.app.referee(&author, &paper.id).await.unwrap(),
        w.app.referee(&coauthor, &paper.id).await.unwrap()
    );
    assert!(matches!(
        w.app.referee(&stranger, &paper.id).await,
        Err(CoreError::NotFound { .. })
    ));
    let author_view =
        serde_json::to_value(w.app.referee(&author, &paper.id).await.unwrap()).unwrap();
    let r = &author_view["rounds"][0];
    assert!(r.get("advice").is_none());
    assert!(r.get("letter").is_none());
    assert!(r["referee"].get("engine").is_none());
    assert_eq!(r["audit"]["state"]["result"]["verdict"], "major_revision");
    assert_eq!(r["referee"]["state"]["result"]["text"], "full answer");
    let attempt = &r["formal"]["state"]["result"]["attempts"][0];
    assert_eq!(attempt["outcome"], "compiled");
    assert_eq!(attempt["theorem"], "Wishpool.C1.main");
    for field in ["lean", "note", "log", "axioms"] {
        assert!(attempt.get(field).is_none());
    }
}

#[tokio::test]
async fn audited_escape_alone_decides_and_reports_and_records_are_idempotent() {
    for (correctness, shape, known, recommendation, accepted) in [
        (
            Correctness::Correct,
            ProofShape::Content,
            None,
            Recommendation::Reject,
            true,
        ),
        (
            Correctness::Correct,
            ProofShape::Content,
            Some("A named prior work".to_string()),
            Recommendation::Accept,
            false,
        ),
        (
            Correctness::Correct,
            ProofShape::BindOnly,
            None,
            Recommendation::Accept,
            false,
        ),
        (
            Correctness::Gap,
            ProofShape::Content,
            None,
            Recommendation::Accept,
            false,
        ),
        (
            Correctness::Error,
            ProofShape::Content,
            None,
            Recommendation::Accept,
            false,
        ),
        (
            Correctness::NotChecked,
            ProofShape::Content,
            None,
            Recommendation::Accept,
            false,
        ),
    ] {
        let w = World::new().await;
        let author = w.person("author", &[]).await;
        let auditor = w.person("wishpool:auditor", &[Role::Reviewer]).await;
        let paper = w.submit(&author, "t", false).await;
        let mut paper = w.review_paper(&author, &paper).await;
        if matches!(
            correctness,
            Correctness::Gap | Correctness::Error | Correctness::NotChecked
        ) {
            paper.claims[0].settles = Some(OpenProblemRef {
                name: "A named problem".into(),
                source: Source {
                    kind: SourceKind::Personal,
                    locator: "Paper".into(),
                    year: None,
                },
            });
            w.app.save(&mut paper).await.unwrap();
        }
        w.app
            .begin_referee_round(&auditor, &paper.id)
            .await
            .unwrap();
        w.app
            .update_referee_round(
                &auditor,
                &paper.id,
                1,
                RoundUpdate::Referee(done_step(referee_report(Some(recommendation)))),
            )
            .await
            .unwrap();
        let result = audited(
            &w,
            &auditor,
            &paper,
            audit_for(&paper, recommendation, correctness, shape, known.clone()),
        )
        .await;
        assert_eq!(
            matches!(result.status, SubmissionStatus::Accepted { .. }),
            accepted
        );
        let literature = result.latest_report(Stage::Literature).unwrap();
        assert_eq!(literature.claims_revision, paper.claims_revision);
        assert!(
            matches!(&literature.reviewer, ReviewerIdentity::Machine { account, engine, .. } if account == &auditor.person && engine == "codex-cli")
        );
        let StagePayload::Literature { prior, .. } = &literature.payload else {
            panic!()
        };
        assert_eq!(prior.len(), usize::from(known.is_some()));
        let StagePayload::Escape { assessments } =
            &result.latest_report(Stage::Escape).unwrap().payload
        else {
            panic!()
        };
        assert_eq!(assessments.len(), 1);
        if correctness != Correctness::Correct {
            assert!(assessments[0].witnesses.is_empty());
        }
        if let Some(Decision::NotAccepted { reasons }) = &result.decision {
            assert!(
                reasons
                    .iter()
                    .any(|r| matches!(r, RejectReason::KnownResult { .. }))
                    == known.is_some()
            );
            assert!(!reasons.is_empty());
        }
        let replay = w
            .app
            .apply_referee_audit(&auditor, &paper.id, 1)
            .await
            .unwrap();
        assert_eq!(result, replay);
        assert_eq!(
            w.stores
                .ports(Arc::new(SystemClock), Arc::new(FakeReader))
                .records
                .list(100, None)
                .await
                .unwrap()
                .items
                .len(),
            usize::from(accepted)
        );
        if !accepted {
            w.app
                .update_referee_round(
                    &auditor,
                    &paper.id,
                    1,
                    RoundUpdate::Advice(done_step(Advice {
                        summary: "help".into(),
                        improvements: vec![],
                        formalization: vec![],
                    })),
                )
                .await
                .unwrap();
            w.app
                .update_referee_round(
                    &auditor,
                    &paper.id,
                    1,
                    RoundUpdate::Letter(done_step(LetterDraft {
                        subject: "Review".into(),
                        body: "Not accepted.".into(),
                        note: String::new(),
                    })),
                )
                .await
                .unwrap();
            assert!(
                w.app
                    .update_referee_round(
                        &auditor,
                        &paper.id,
                        1,
                        RoundUpdate::Formal(done_step(FormalProbe {
                            toolchain: "lean".into(),
                            attempts: vec![],
                            summary: String::new()
                        }))
                    )
                    .await
                    .is_err()
            );
            w.app
                .update_referee_round(
                    &auditor,
                    &paper.id,
                    1,
                    RoundUpdate::Formal(Step {
                        state: StepState::Skipped {
                            reason: "not accepted".into(),
                        },
                        ..Step::pending()
                    }),
                )
                .await
                .unwrap();
        }
    }
}

#[tokio::test]
async fn audit_recovers_record_insert_and_preserves_publication_on_revision() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let auditor = w.person("auditor", &[Role::Reviewer]).await;
    let paper = w.submit(&author, "tc", false).await;
    let paper = w.review_paper(&author, &paper).await;
    w.app
        .begin_referee_round(&auditor, &paper.id)
        .await
        .unwrap();
    w.app
        .update_referee_round(
            &auditor,
            &paper.id,
            1,
            RoundUpdate::Referee(done_step(referee_report(Some(Recommendation::Reject)))),
        )
        .await
        .unwrap();
    let accepted = audited(
        &w,
        &auditor,
        &paper,
        audit_for(
            &paper,
            Recommendation::MinorRevision,
            Correctness::Correct,
            ProofShape::Content,
            None,
        ),
    )
    .await;
    let SubmissionStatus::Accepted { record } = &accepted.status else {
        panic!()
    };
    // Simulate a crash after record insertion, before saving the final status.
    let mut before_status = accepted.clone();
    before_status.status = SubmissionStatus::InReview;
    before_status.decision = None;
    w.app.save(&mut before_status).await.unwrap();
    let recovered = w
        .app
        .apply_referee_audit(&auditor, &paper.id, 1)
        .await
        .unwrap();
    assert_eq!(recovered.status, accepted.status);
    assert_eq!(recovered.reports, accepted.reports);
    let records = w
        .stores
        .ports(Arc::new(SystemClock), Arc::new(FakeReader))
        .records;
    assert_eq!(records.list(100, None).await.unwrap().items.len(), 1);
    let immutable = records.get(record).await.unwrap().unwrap();
    let editor = w.person("editor", &[Role::Editor]).await;
    let claim = &paper.claims[0].id;
    w.app
        .propose_formalization(&editor, &paper.id, claim, "A useful proof".into())
        .await
        .unwrap();
    w.app
        .respond_to_formalization(&author, &paper.id, claim, true, String::new())
        .await
        .unwrap();
    w.app
        .verify_formalization(
            &editor,
            &paper.id,
            claim,
            FormalArtifact {
                repository: "https://github.com/example/paper".into(),
                commit: "a".repeat(40),
                declarations: vec!["Original.main".into()],
            },
            vec![],
            None,
        )
        .await
        .unwrap();
    w.app
        .update_conjecture(
            &editor,
            &paper.id,
            &paper.claims[1].id,
            ConjectureState::NotPursued {
                reason: "No further approach yet".into(),
            },
        )
        .await
        .unwrap();
    w.app
        .set_analysis_visibility(&author, &paper.id, Visibility::Public)
        .await
        .unwrap();
    let original = w.app.paper(record).await.unwrap();
    assert!(original.claims[0].lean.is_some());
    assert!(matches!(
        w.app
            .submission(&author, &paper.id)
            .await
            .unwrap()
            .conjectures[0]
            .state,
        ConjectureState::NotPursued { .. }
    ));
    // Public downloads use the pinned blob even if a later worker recompiles it.
    w.app
        .record_compilation(&auditor, &paper.id, 1, Ok(b"%PDF recompiled".to_vec()))
        .await
        .unwrap();
    assert_eq!(
        w.app
            .paper_file(None, &paper.id, Some(1), true)
            .await
            .unwrap()
            .bytes,
        b"%PDF"
    );
    let revised = w
        .app
        .upload_version(
            &author,
            &paper.id,
            Upload {
                bytes: b"new source".to_vec(),
                filename: "tl.tex".into(),
            },
            "clarified proof".into(),
        )
        .await
        .unwrap();
    assert_eq!(revised.status, SubmissionStatus::Draft);
    assert!(revised.formalization.items.is_empty());
    assert!(revised.conjectures.is_empty());
    assert_eq!(w.app.paper(record).await.unwrap(), original);
    assert_eq!(records.get(record).await.unwrap().unwrap(), immutable);
    assert!(
        w.app
            .paper_file(None, &paper.id, Some(2), true)
            .await
            .is_err()
    );
    assert_eq!(
        w.app
            .paper_file(None, &paper.id, None, true)
            .await
            .unwrap()
            .bytes,
        b"%PDF"
    );
    assert!(
        w.app
            .apply_referee_audit(&auditor, &paper.id, 1)
            .await
            .is_err()
    );
    // Reused statement IDs belong to the new inputs, never to the old proof.
    let confirmed = w.review_version(&author, &revised, 2).await;
    assert!(confirmed.formalization.items.is_empty());
    assert_eq!(w.app.paper(record).await.unwrap(), original);
}

#[tokio::test]
async fn legacy_publication_can_be_revised_without_mutating_its_record() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let editor = w.person("editor", &[Role::Editor]).await;
    let paper = w.submit(&author, "t", false).await;
    let mut paper = w.review_paper(&author, &paper).await;
    let record = Record {
        kind: SubmissionKind::Paper,
        id: RecordId::from("WP-2026-0099"),
        publication: None,
        submission: paper.id.clone(),
        title: paper.title.clone(),
        authors: paper.authors.clone(),
        basis: AdmissionBasis::EscapeWitness,
        accepted_by: editor.person.clone(),
        accepted_at: w.app.ports.clock.now(),
    };
    let records = w
        .stores
        .ports(Arc::new(SystemClock), Arc::new(FakeReader))
        .records;
    records.insert(&record).await.unwrap();
    paper.status = SubmissionStatus::Accepted {
        record: record.id.clone(),
    };
    w.app.save(&mut paper).await.unwrap();
    let original = w.app.paper(&record.id).await.unwrap();
    let revised = w
        .app
        .upload_version(
            &author,
            &paper.id,
            Upload {
                bytes: b"new source".to_vec(),
                filename: "tl.tex".into(),
            },
            "a revised argument".into(),
        )
        .await
        .unwrap();
    assert_eq!(revised.status, SubmissionStatus::Draft);
    assert_eq!(w.app.paper(&record.id).await.unwrap(), original);
    assert_eq!(records.get(&record.id).await.unwrap().unwrap(), record);
    assert_eq!(
        w.app
            .paper_file(None, &paper.id, None, true)
            .await
            .unwrap()
            .bytes,
        b"%PDF"
    );
    assert!(
        w.app
            .paper_file(None, &paper.id, Some(2), true)
            .await
            .is_err()
    );
    let confirmed = w.review_version(&author, &revised, 2).await;
    assert!(confirmed.formalization.items.is_empty());
    assert_eq!(w.app.paper(&record.id).await.unwrap(), original);
    // A failed second review and another upload still retain the original inputs.
    let mut declined = confirmed;
    declined.status = SubmissionStatus::NotAccepted;
    w.app.save(&mut declined).await.unwrap();
    w.app
        .upload_version(
            &author,
            &paper.id,
            Upload {
                bytes: b"third source".to_vec(),
                filename: "tlc.tex".into(),
            },
            "another revision".into(),
        )
        .await
        .unwrap();
    assert_eq!(w.app.paper(&record.id).await.unwrap(), original);
}

#[tokio::test]
async fn no_main_result_and_legacy_rounds() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let auditor = w.person("auditor", &[Role::Reviewer]).await;
    let paper = w.submit(&author, "t", false).await;
    let mut paper = w.review_paper(&author, &paper).await;
    paper.claims[0].role = ClaimRole::Supporting;
    w.app.save(&mut paper).await.unwrap();
    let file = w
        .app
        .begin_referee_round(&auditor, &paper.id)
        .await
        .unwrap();
    let mut legacy = serde_json::to_value(file.current().unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("audit");
    let legacy: RefereeRound = serde_json::from_value(legacy).unwrap();
    assert!(
        matches!(legacy.audit.state, StepState::Skipped { reason } if reason == "not part of this round")
    );
    w.app
        .update_referee_round(
            &auditor,
            &paper.id,
            1,
            RoundUpdate::Referee(done_step(referee_report(Some(Recommendation::Accept)))),
        )
        .await
        .unwrap();
    let result = audited(
        &w,
        &auditor,
        &paper,
        audit_for(
            &paper,
            Recommendation::Accept,
            Correctness::Correct,
            ProofShape::Content,
            None,
        ),
    )
    .await;
    assert_eq!(
        result.decision,
        Some(Decision::NotAccepted {
            reasons: vec![RejectReason::NoMainResult]
        })
    );
}

#[tokio::test]
async fn failed_referee_cannot_produce_audit_advice_or_letter() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("auditor", &[Role::Reviewer]).await;
    let paper = w.submit(&author, "t", false).await;
    let paper = w.review_paper(&author, &paper).await;
    w.app
        .begin_referee_round(&reviewer, &paper.id)
        .await
        .unwrap();
    let failure = Step {
        state: StepState::Failed {
            reason: "failure".into(),
            detail: None,
            retryable: false,
            at: chrono::Utc::now(),
        },
        ..Step::pending()
    };
    w.app
        .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Referee(failure))
        .await
        .unwrap();
    assert!(
        w.app
            .update_referee_round(
                &reviewer,
                &paper.id,
                1,
                RoundUpdate::Audit(done_step(audit_for(
                    &paper,
                    Recommendation::Accept,
                    Correctness::Correct,
                    ProofShape::Content,
                    None
                )))
            )
            .await
            .is_err()
    );
    assert!(
        w.app
            .apply_referee_audit(&reviewer, &paper.id, 1)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn referee_updates_fence_version_and_claims_revision() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("referee", &[Role::Reviewer]).await;
    let paper = w.submit(&author, "t", false).await;
    let paper = w.review_paper(&author, &paper).await;
    w.app
        .begin_referee_round(&reviewer, &paper.id)
        .await
        .unwrap();
    let mut changed = paper.clone();
    changed.versions.last_mut().unwrap().number += 1;
    w.app.save(&mut changed).await.unwrap();
    assert!(matches!(
        w.app
            .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Audit(Step::pending()))
            .await,
        Err(CoreError::Conflict(_))
    ));
    changed.versions.last_mut().unwrap().number -= 1;
    changed.claims_revision += 1;
    w.app.save(&mut changed).await.unwrap();
    assert!(matches!(
        w.app
            .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Audit(Step::pending()))
            .await,
        Err(CoreError::Conflict(_))
    ));
}

#[tokio::test]
async fn referee_visibility_restart_and_letter_editing() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let coauthor = w.person("coauthor", &[]).await;
    let stranger = w.person("stranger", &[]).await;
    let reviewer = w.person("referee", &[Role::Reviewer]).await;
    let editor = w.person("editor", &[Role::Editor]).await;
    let paper = w.submit(&author, "t", false).await;
    let mut paper = w.review_paper(&author, &paper).await;
    paper.authors[0].person = Some(coauthor.person.clone());
    w.app.save(&mut paper).await.unwrap();
    w.app
        .begin_referee_round(&reviewer, &paper.id)
        .await
        .unwrap();
    assert!(w.app.restart_referee(&editor, &paper.id).await.is_err());
    assert!(w.app.restart_referee(&author, &paper.id).await.is_err());
    assert!(
        w.app
            .referee(&author, &paper.id)
            .await
            .unwrap()
            .rounds
            .len()
            == 1
    );
    assert!(matches!(
        w.app.referee(&stranger, &paper.id).await,
        Err(CoreError::NotFound { .. })
    ));
    w.app
        .update_referee_round(
            &reviewer,
            &paper.id,
            1,
            RoundUpdate::Referee(done_step(referee_report(Some(Recommendation::Reject)))),
        )
        .await
        .unwrap();
    w.app
        .update_referee_round(
            &reviewer,
            &paper.id,
            1,
            RoundUpdate::Audit(Step {
                state: StepState::Skipped {
                    reason: "failed audit".into(),
                },
                ..Step::pending()
            }),
        )
        .await
        .unwrap();
    let mut skip = Step::pending();
    skip.state = StepState::Skipped {
        reason: "not positive".into(),
    };
    w.app
        .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Advice(skip))
        .await
        .unwrap();
    assert!(matches!(
        w.app
            .update_referee_round(
                &reviewer,
                &paper.id,
                1,
                RoundUpdate::Formal(done_step(FormalProbe {
                    toolchain: "lean".into(),
                    attempts: vec![],
                    summary: String::new(),
                }))
            )
            .await,
        Err(CoreError::Conflict(_))
    ));
    let draft = LetterDraft {
        subject: "Feedback".into(),
        body: "A precise gap.".into(),
        note: "Longer argument".into(),
    };
    w.app
        .update_referee_round(
            &reviewer,
            &paper.id,
            1,
            RoundUpdate::Letter(Step {
                state: StepState::Skipped {
                    reason: "no audit".into(),
                },
                ..Step::pending()
            }),
        )
        .await
        .unwrap();
    w.app
        .update_referee_round(
            &reviewer,
            &paper.id,
            1,
            RoundUpdate::Formal(Step {
                state: StepState::Skipped {
                    reason: "no advice".into(),
                },
                ..Step::pending()
            }),
        )
        .await
        .unwrap();
    let new = NewLetter {
        assessment: Some(Recommendation::MajorRevision),
        subject: draft.subject.clone(),
        body: draft.body.clone(),
        note: draft.note.clone(),
    };
    assert!(
        w.app
            .send_feedback(&author, &paper.id, new.clone())
            .await
            .is_err()
    );
    assert!(
        w.app
            .send_feedback(&editor, &paper.id, new.clone())
            .await
            .unwrap()
            .edited
    );
    for field in ["subject", "body", "note"] {
        let mut edited = new.clone();
        match field {
            "subject" => edited.subject.push('!'),
            "body" => edited.body.push('!'),
            _ => edited.note.push('!'),
        }
        assert!(
            w.app
                .send_feedback(&editor, &paper.id, edited)
                .await
                .unwrap()
                .edited
        );
    }
    for caller in [&author, &coauthor] {
        let view = w.app.referee(caller, &paper.id).await.unwrap();
        assert_eq!(view.rounds.len(), 1);
        assert!(view.rounds[0].advice.is_none() && view.rounds[0].letter.is_none());
        assert_eq!(view.rounds[0].referee.done().unwrap().text, "full answer");
        assert_eq!(view.letters.len(), 4);
        assert!(
            view.letters
                .iter()
                .all(|letter| { letter.assessment == Some(Recommendation::MajorRevision) })
        );
    }
    assert_eq!(w.app.submission(&author, &paper.id).await.unwrap(), paper);
    assert_eq!(
        w.app
            .referee(&editor, &paper.id)
            .await
            .unwrap()
            .rounds
            .len(),
        1
    );
    assert_eq!(
        w.app
            .restart_referee(&editor, &paper.id)
            .await
            .unwrap()
            .rounds
            .len(),
        2
    );
}

#[tokio::test]
async fn sent_letter_validation_and_statuses() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let editor = w.person("editor", &[Role::Editor]).await;
    let mut paper = w.submit(&author, "t", false).await;
    let new = NewLetter {
        assessment: None,
        subject: "s".into(),
        body: "b".into(),
        note: String::new(),
    };
    for invalid in [
        NewLetter {
            body: " \n\t".into(),
            ..new.clone()
        },
        NewLetter {
            subject: "界".repeat(301),
            ..new.clone()
        },
        NewLetter {
            body: "x".repeat(MAX_LETTER_CHARS + 1),
            ..new.clone()
        },
        NewLetter {
            note: "x".repeat(MAX_LETTER_CHARS + 1),
            ..new.clone()
        },
    ] {
        assert!(matches!(
            w.app.send_feedback(&editor, &paper.id, invalid).await,
            Err(CoreError::Invalid(_))
        ));
    }
    for status in [
        SubmissionStatus::Draft,
        SubmissionStatus::InReview,
        SubmissionStatus::NotAccepted,
        SubmissionStatus::Accepted {
            record: RecordId::from("WP-2026-0001"),
        },
    ] {
        paper.status = status;
        w.app.save(&mut paper).await.unwrap();
        let letter = w
            .app
            .send_feedback(&editor, &paper.id, new.clone())
            .await
            .unwrap();
        assert!(letter.edited);
        assert_eq!(letter.round, None);
    }
    paper.status = SubmissionStatus::Withdrawn;
    w.app.save(&mut paper).await.unwrap();
    assert!(matches!(
        w.app.send_feedback(&editor, &paper.id, new).await,
        Err(CoreError::Conflict(_))
    ));
}

fn open_reading() -> ConjectureReading {
    ConjectureReading {
        well_posed: true,
        well_posed_reason: "All symbols and quantifiers are defined.".into(),
        status: ConjectureStatus::Open,
        status_reason: "No solution in the available material.".into(),
        named_works: vec![],
        escape: ProofShape::Content,
        escape_reason: "A proof would require a new estimate.".into(),
        suggestions: vec!["Explain the boundary case.".into()],
    }
}

async fn displayed_conjecture(w: &World, author: &Caller, reviewer: &Caller) -> Submission {
    let mut paper = w.submit(author, "c", false).await;
    paper.kind = SubmissionKind::Conjecture;
    paper.extracted[0].role = ClaimRole::Main;
    w.app.save(&mut paper).await.unwrap();
    let paper = w.review_paper(author, &paper).await;
    w.app
        .begin_referee_round(reviewer, &paper.id)
        .await
        .unwrap();
    w.app
        .update_referee_round(
            reviewer,
            &paper.id,
            1,
            RoundUpdate::Referee(done_step(referee_report(Some(Recommendation::Accept)))),
        )
        .await
        .unwrap();
    let audit = RefereeAudit {
        verdict: Recommendation::Accept,
        agrees_with_referee: true,
        summary: "PRIVATE_AUDIT_SUMMARY".into(),
        claims: vec![AuditedClaim {
            claim: "C1".into(),
            correctness: Correctness::NotChecked,
            comment: "PRIVATE_CLAIM_COMMENT".into(),
            shape: None,
            witnesses: vec![],
            known: None,
            referee_agreed: true,
            conjecture: Some(open_reading()),
        }],
        concerns: vec![],
    };
    let paper = audited(w, reviewer, &paper, audit).await;
    w.app
        .update_referee_round(
            reviewer,
            &paper.id,
            1,
            RoundUpdate::Advice(Step {
                state: StepState::Skipped {
                    reason: "no advice".into(),
                },
                ..Step::pending()
            }),
        )
        .await
        .unwrap();
    w.app
        .update_referee_round(
            reviewer,
            &paper.id,
            1,
            RoundUpdate::Letter(done_step(LetterDraft {
                subject: "Display".into(),
                body: "Dear Author,\n\nPRIVATE_LETTER_TEXT".into(),
                note: String::new(),
            })),
        )
        .await
        .unwrap();
    paper
}

#[tokio::test]
async fn lean_statement_confirmation_is_cookie_only_digest_bound_and_voided_on_revision() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("reviewer", &[Role::Reviewer]).await;
    let paper = displayed_conjecture(&w, &author, &reviewer).await;
    let record = match &paper.status {
        SubmissionStatus::Accepted { record } => record,
        _ => panic!(),
    };
    assert!(
        w.app.referee(&author, &paper.id).await.unwrap().letters[0]
            .body
            .contains("Your conjecture is displayed")
    );
    w.app
        .queue_lean_statements(&reviewer, &paper.id)
        .await
        .unwrap();
    let first = w
        .app
        .record_lean_statement(
            &reviewer,
            &paper.id,
            1,
            paper.claims_revision,
            &"C1".into(),
            "import Mathlib\ndef wishpool_target_prop : Prop := True\n".into(),
            "Lean test".into(),
            "The conjecture says True.".into(),
            None,
        )
        .await
        .unwrap();
    let digest = first.lean_statements[0].digest.clone();
    assert_eq!(digest.len(), 64);
    assert!(
        w.app
            .respond_lean_statement(
                &author,
                AuthenticationMethod::Bearer,
                &paper.id,
                &digest,
                true,
                String::new()
            )
            .await
            .is_err()
    );
    assert!(
        w.app
            .respond_lean_statement(
                &author,
                AuthenticationMethod::CookieSession,
                &paper.id,
                "different-digest",
                true,
                String::new()
            )
            .await
            .is_err()
    );
    assert!(
        w.app
            .respond_lean_statement(
                &author,
                AuthenticationMethod::CookieSession,
                &paper.id,
                &digest,
                false,
                " ".into()
            )
            .await
            .is_err()
    );
    w.app
        .respond_lean_statement(
            &author,
            AuthenticationMethod::CookieSession,
            &paper.id,
            &digest,
            false,
            "Use the original quantifier".into(),
        )
        .await
        .unwrap();
    assert!(
        w.app
            .respond_lean_statement(
                &author,
                AuthenticationMethod::CookieSession,
                &paper.id,
                &digest,
                true,
                String::new()
            )
            .await
            .is_err()
    );
    let second = w
        .app
        .record_lean_statement(
            &reviewer,
            &paper.id,
            1,
            paper.claims_revision,
            &"C1".into(),
            "import Mathlib\ndef wishpool_target_prop : Prop := ∀ n : Nat, n = n\n".into(),
            "Lean test".into(),
            "Every natural number equals itself.".into(),
            Some(digest),
        )
        .await
        .unwrap();
    let digest = second.lean_statements[1].digest.clone();
    w.app
        .respond_lean_statement(
            &author,
            AuthenticationMethod::CookieSession,
            &paper.id,
            &digest,
            true,
            String::new(),
        )
        .await
        .unwrap();
    assert_eq!(w.app.paper(record).await.unwrap().lean_statements.len(), 1);
    assert_eq!(
        w.app.list_conjectures(None, None).await.unwrap().items[0].lean_statement_status,
        super::records::LeanStatementStatus::Confirmed
    );
    let revised = w
        .app
        .upload_version(
            &author,
            &paper.id,
            Upload {
                filename: "c.tex".into(),
                bytes: b"new source".to_vec(),
            },
            "Revised".into(),
        )
        .await
        .unwrap();
    assert!(revised.lean_statements.is_empty());
    assert!(
        w.app
            .respond_lean_statement(
                &author,
                AuthenticationMethod::CookieSession,
                &paper.id,
                &digest,
                true,
                String::new()
            )
            .await
            .is_err()
    );
    assert!(
        w.app
            .paper(record)
            .await
            .unwrap()
            .lean_statements
            .is_empty()
    );
}

#[tokio::test]
async fn public_projection_serialization_excludes_all_private_reviews_and_rationales() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("auditor", &[Role::Reviewer]).await;
    let paper = w.submit(&author, "tl", false).await;
    let paper = w.review_paper(&author, &paper).await;
    w.app
        .begin_referee_round(&reviewer, &paper.id)
        .await
        .unwrap();
    let mut report = referee_report(Some(Recommendation::Accept));
    report.summary = "PRIVATE_REPORT_SUMMARY".into();
    report.text = "PRIVATE_GPT_REPORT".into();
    w.app
        .update_referee_round(
            &reviewer,
            &paper.id,
            1,
            RoundUpdate::Referee(done_step(report)),
        )
        .await
        .unwrap();
    let mut audit = audit_for(
        &paper,
        Recommendation::Accept,
        Correctness::Correct,
        ProofShape::Content,
        None,
    );
    audit.summary = "PRIVATE_AUDIT_SUMMARY".into();
    audit.claims[0].comment = "PRIVATE_COMMENT".into();
    let paper = audited(&w, &reviewer, &paper, audit).await;
    let record = match &paper.status {
        SubmissionStatus::Accepted { record } => record,
        _ => panic!(),
    };
    let payload = w.app.paper(record).await.unwrap();
    assert_eq!(payload.summary.new_results, 1);
    assert_eq!(payload.new_content.len(), 1);
    assert_eq!(payload.new_content[0].lemmas, ["The gap estimate"]);
    let text = serde_json::to_string(&payload).unwrap();
    for forbidden in [
        "correctness",
        "comment",
        "rationale",
        "analysis",
        "audit_summary",
        "referee",
        "letter",
        "advice",
        "PRIVATE_",
        "Correct:",
        "not_checked",
        "gap\"",
    ] {
        assert!(!text.contains(forbidden), "leaked {forbidden}: {text}");
    }
    w.app
        .set_analysis_visibility(&author, &paper.id, Visibility::Private)
        .await
        .unwrap();
    let private = serde_json::to_value(w.app.paper(record).await.unwrap()).unwrap();
    assert_eq!(
        private.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["summary"]
    );
    let fields = private["summary"].as_object().unwrap();
    assert_eq!(
        fields.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        BTreeSet::from(["authors", "kind", "record", "title"])
    );
    assert!(w.app.paper_file(None, &paper.id, None, true).await.is_err());
}

#[tokio::test]
async fn upload_checkbox_maps_to_visibility() {
    for checked in [false, true] {
        let w = World::new().await;
        let author = w.person("author", &[]).await;
        let mut new: NewPaper = serde_json::from_value(
            serde_json::json!({"ai_disclosure":{"level":"none","statement":"No AI used."}}),
        )
        .unwrap();
        new.make_public_after_acceptance = checked;
        let s = w
            .app
            .submit_paper(
                &author,
                new,
                Upload {
                    filename: "t.tex".into(),
                    bytes: b"source".to_vec(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            s.analysis_visibility,
            if checked {
                Visibility::Public
            } else {
                Visibility::Private
            }
        );
    }
}

#[tokio::test]
async fn conjecture_listing_pages_past_other_kinds_and_private_records_in_record_order() {
    use crate::ports::RecordStore;
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("reviewer", &[Role::Reviewer]).await;
    let original = displayed_conjecture(&w, &author, &reviewer).await;
    let original_record = w
        .stores
        .for_submission(&original.id)
        .await
        .unwrap()
        .unwrap();
    for (index, kind, visibility) in [
        (2, SubmissionKind::Conjecture, Visibility::Public),
        (3, SubmissionKind::Paper, Visibility::Public),
        (4, SubmissionKind::Conjecture, Visibility::Private),
        (5, SubmissionKind::Note, Visibility::Public),
        (6, SubmissionKind::Conjecture, Visibility::Undecided),
        (7, SubmissionKind::Conjecture, Visibility::Public),
    ] {
        let mut publication = original.clone();
        publication.id = format!("listing-{index}").as_str().into();
        publication.kind = kind;
        publication.analysis_visibility = visibility;
        publication.claims[0].statement = format!("For every n,\n  property {index} holds.");
        publication.title = format!("Submission {index}");
        publication.status = SubmissionStatus::Accepted {
            record: format!("WP-2026-{index:04}").as_str().into(),
        };
        w.app.ports.submissions.insert(&publication).await.unwrap();
        let mut record = original_record.clone();
        record.id = format!("WP-2026-{index:04}").as_str().into();
        record.kind = kind;
        record.submission = publication.id.clone();
        record.title = publication.title.clone();
        record.publication = Some(Box::new(publication));
        w.stores.insert(&record).await.unwrap();
    }
    let first = w.app.list_conjectures(Some(2), None).await.unwrap();
    assert_eq!(
        first
            .items
            .iter()
            .map(|c| c.record.as_str())
            .collect::<Vec<_>>(),
        ["WP-2026-0007", "WP-2026-0002"]
    );
    assert_eq!(first.items[0].statement, "For every n, property 7 holds.");
    assert_eq!(
        first.items[0].lean_statement_status,
        super::records::LeanStatementStatus::None
    );
    let second = w
        .app
        .list_conjectures(Some(2), first.next_before)
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert_eq!(second.items[0].record, original_record.id);
    assert!(second.next_before.is_none());
}

mod solving;
