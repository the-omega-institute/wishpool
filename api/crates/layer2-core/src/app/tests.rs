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
            summary: "Searched OpenAlex and arXiv.".into(),
            payload: StagePayload::Literature {
                prior,
                searched: vec!["OpenAlex: zero runs gaps".into()],
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

    // Readers see the paper; the analysis waits for the author's choice.
    let record = RecordId::format(year, 1);
    let public = w.app.paper(&record).await.unwrap();
    assert_eq!(public.summary.doi.as_deref(), Some("10.1000/xyz"));
    assert_eq!(public.claims.len(), 3);
    assert!(public.analysis.is_none() && public.conjectures.is_empty());
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
    let analysis = w.app.paper(&record).await.unwrap().analysis.unwrap();
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
        w.app.paper(&record).await.unwrap().conjectures[0].state,
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

#[tokio::test]
async fn round_ordering_and_settled_immutability() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("referee", &[Role::Reviewer]).await;
    let paper = w.submit(&author, "t", false).await;
    let paper = w.review_paper(&author, &paper).await;
    w.app
        .begin_referee_round(&reviewer, &paper.id)
        .await
        .unwrap();
    let claim = paper.claims[0].id.clone();
    let advice = done_step(Advice {
        summary: "help".into(),
        improvements: vec![],
        formalization: vec![FormalizationCandidate {
            claim: claim.clone(),
            feasibility: Feasibility::Ready,
            mathlib: vec![],
            missing: vec![],
            lean_sketch: String::new(),
            plan: "state and prove".into(),
            effort: Effort::Small,
        }],
    });
    let attempt = |claim: ClaimId| FormalAttempt {
        claim,
        outcome: ProbeOutcome::Compiled,
        theorem: Some("wishpool_c1".into()),
        lean: "import Mathlib".into(),
        axioms: vec!["propext".into()],
        note: String::new(),
        log: String::new(),
    };
    let probe = |claim: ClaimId| {
        done_step(FormalProbe {
            toolchain: "leanprover/lean4:v4.33.0".into(),
            attempts: vec![attempt(claim)],
            summary: String::new(),
        })
    };
    let formal = probe(claim.clone());
    let letter = done_step(LetterDraft {
        subject: "Review".into(),
        body: "Useful feedback.".into(),
        note: "Argument".into(),
    });
    for update in [
        RoundUpdate::Advice(advice.clone()),
        RoundUpdate::Formal(formal.clone()),
        RoundUpdate::Letter(letter.clone()),
    ] {
        assert!(matches!(
            w.app
                .update_referee_round(&reviewer, &paper.id, 1, update)
                .await,
            Err(CoreError::Conflict(_))
        ));
    }
    let report = done_step(referee_report(Some(Recommendation::Accept)));
    w.app
        .update_referee_round(
            &reviewer,
            &paper.id,
            1,
            RoundUpdate::Referee(report.clone()),
        )
        .await
        .unwrap();
    assert!(
        w.app
            .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Letter(letter.clone()))
            .await
            .is_err()
    );
    w.app
        .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Advice(advice.clone()))
        .await
        .unwrap();
    assert!(
        w.app
            .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Letter(letter.clone()))
            .await
            .is_err()
    );
    assert!(matches!(
        w.app
            .update_referee_round(
                &reviewer,
                &paper.id,
                1,
                RoundUpdate::Formal(probe("C99".into()))
            )
            .await,
        Err(CoreError::Conflict(_))
    ));
    w.app
        .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Formal(formal.clone()))
        .await
        .unwrap();
    w.app
        .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Letter(letter.clone()))
        .await
        .unwrap();
    for update in [
        RoundUpdate::Referee(report),
        RoundUpdate::Advice(advice),
        RoundUpdate::Formal(formal),
        RoundUpdate::Letter(letter),
    ] {
        assert!(matches!(
            w.app
                .update_referee_round(&reviewer, &paper.id, 1, update)
                .await,
            Err(CoreError::Conflict(_))
        ));
    }
}

#[tokio::test]
async fn advice_requires_positive_and_negative_referee_is_skipped() {
    for recommendation in [
        Some(Recommendation::Accept),
        Some(Recommendation::MinorRevision),
        Some(Recommendation::MajorRevision),
        Some(Recommendation::Reject),
        None,
    ] {
        let w = World::new().await;
        let author = w.person("author", &[]).await;
        let reviewer = w.person("referee", &[Role::Reviewer]).await;
        let paper = w.submit(&author, "t", false).await;
        let paper = w.review_paper(&author, &paper).await;
        w.app
            .begin_referee_round(&reviewer, &paper.id)
            .await
            .unwrap();
        w.app
            .update_referee_round(
                &reviewer,
                &paper.id,
                1,
                RoundUpdate::Referee(done_step(referee_report(recommendation))),
            )
            .await
            .unwrap();
        let positive = recommendation.is_some_and(Recommendation::is_positive);
        let done = done_step(Advice {
            summary: "help".into(),
            improvements: vec![],
            formalization: vec![],
        });
        assert_eq!(
            w.app
                .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Advice(done))
                .await
                .is_ok(),
            positive
        );
        if !positive {
            let mut running = Step::pending();
            running.state = StepState::Running {
                task: None,
                queue_position: None,
                since: chrono::Utc::now(),
            };
            assert!(
                w.app
                    .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Advice(running))
                    .await
                    .is_err()
            );
            let mut skip = Step::pending();
            skip.state = StepState::Skipped {
                reason: "not positive".into(),
            };
            w.app
                .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Advice(skip.clone()))
                .await
                .unwrap();
            assert!(
                w.app
                    .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Advice(skip))
                    .await
                    .is_err()
            );
        }
    }
}

#[tokio::test]
async fn failed_referee_cannot_produce_advice_or_letter() {
    let w = World::new().await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("referee", &[Role::Reviewer]).await;
    let paper = w.submit(&author, "t", false).await;
    let paper = w.review_paper(&author, &paper).await;
    w.app
        .begin_referee_round(&reviewer, &paper.id)
        .await
        .unwrap();
    let mut failure = Step::pending();
    failure.state = StepState::Failed {
        reason: "failed".into(),
        detail: Some("detail".into()),
        retryable: true,
        at: chrono::Utc::now(),
    };
    w.app
        .update_referee_round(
            &reviewer,
            &paper.id,
            1,
            RoundUpdate::Referee(failure.clone()),
        )
        .await
        .unwrap();
    assert!(
        w.app
            .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Referee(failure))
            .await
            .is_err()
    );
    let mut bad = Step::pending();
    bad.state = StepState::Failed {
        reason: "bad".into(),
        detail: None,
        retryable: false,
        at: chrono::Utc::now(),
    };
    assert!(
        w.app
            .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Advice(bad))
            .await
            .is_err()
    );
    let mut skip = Step::pending();
    skip.state = StepState::Skipped {
        reason: "failed referee".into(),
    };
    w.app
        .update_referee_round(&reviewer, &paper.id, 1, RoundUpdate::Advice(skip))
        .await
        .unwrap();
    assert!(
        w.app
            .update_referee_round(
                &reviewer,
                &paper.id,
                1,
                RoundUpdate::Letter(done_step(LetterDraft {
                    subject: "x".into(),
                    body: "x".into(),
                    note: String::new()
                }))
            )
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
            .update_referee_round(
                &reviewer,
                &paper.id,
                1,
                RoundUpdate::Referee(Step::pending())
            )
            .await,
        Err(CoreError::Conflict(_))
    ));
    changed.versions.last_mut().unwrap().number -= 1;
    changed.claims_revision += 1;
    w.app.save(&mut changed).await.unwrap();
    assert!(matches!(
        w.app
            .update_referee_round(
                &reviewer,
                &paper.id,
                1,
                RoundUpdate::Referee(Step::pending())
            )
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
            .is_empty()
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
            RoundUpdate::Letter(done_step(draft.clone())),
        )
        .await
        .unwrap();
    let new = NewLetter {
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
        !w.app
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
        assert!(view.rounds.is_empty());
        assert_eq!(view.letters.len(), 4);
    }
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
