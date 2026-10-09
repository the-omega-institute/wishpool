use std::{
    collections::{BTreeSet, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use tokio::sync::Mutex;
use wishpool_core::{
    app::{App, Upload},
    memory::MemoryStores,
    model::{
        AiDisclosure, AiUse, Caller, ClaimConfirmation, NewPaper, Recommendation, ReviewerIdentity,
        Role, StepState, Submission,
    },
    policy::Policy,
    ports::{JobKind, SystemClock},
};
use wishpool_review::{
    ReviewResult,
    advisor::{Advisor, AdvisorInput},
    lean::{CheckedFile, FormalInput, FormalOut, Formalizer},
    oracle::{Oracle, OracleRequest, OracleStatus, OracleSubmitted},
    referee_prompts::{AdviceOut, FormalizationOut, LetterOut},
};

use super::*;
use crate::{
    latex::{Compile, LatexReader},
    review::MemoryJobs,
};

const SOURCE: &str = r"\documentclass{article}
\newtheorem{theorem}{Theorem}
\newtheorem{conjecture}{Conjecture}
\title{A paper}\author{A. Author}
\begin{document}
\begin{theorem}Every gap is finite.\end{theorem}
\begin{proof}By compactness.\end{proof}
\begin{conjecture}Every gap is small.\end{conjecture}
\end{document}";

struct FakeOracle {
    submitted: AtomicUsize,
    references: Mutex<Vec<String>>,
    outcomes: Mutex<VecDeque<ReviewResult<OracleStatus>>>,
    submit_error: AtomicUsize,
}

#[async_trait]
impl Oracle for FakeOracle {
    fn engine(&self) -> &str {
        "nyxid-oracle"
    }
    async fn submit(&self, request: &OracleRequest) -> ReviewResult<OracleSubmitted> {
        self.submitted.fetch_add(1, Ordering::SeqCst);
        self.references
            .lock()
            .await
            .push(request.client_ref.clone());
        assert!(request.prompt.contains("C1") && request.prompt.contains("C2"));
        assert!(request.prompt.contains("\"proved\": false"));
        assert!(request.pdf.is_some());
        if self.submit_error.swap(0, Ordering::SeqCst) != 0 {
            return Err(ReviewError::Transport("uncertain delivery".into()));
        }
        Ok(OracleSubmitted {
            task: "task-1".into(),
            queue_position: Some(2),
        })
    }
    async fn poll(&self, task: &str) -> ReviewResult<OracleStatus> {
        assert_eq!(task, "task-1");
        self.outcomes
            .lock()
            .await
            .pop_front()
            .expect("unexpected poll")
    }
}

#[derive(Default)]
struct FakeAdvisor {
    advice: AtomicUsize,
    letters: AtomicUsize,
    fail_advice: bool,
    fail_letter: bool,
    /// Propose C1 and the open C2 for formalization.
    propose: AtomicBool,
    formal_seen: Mutex<Option<serde_json::Value>>,
}

#[derive(Default)]
struct FakeFormalizer {
    calls: AtomicUsize,
    never_completes: bool,
}

#[async_trait]
impl Formalizer for FakeFormalizer {
    fn engine(&self) -> &str {
        "test-lean"
    }
    fn model(&self) -> &str {
        "codex-test"
    }
    async fn formalize(
        &self,
        input: &FormalInput,
        work: &std::path::Path,
    ) -> ReviewResult<FormalOut> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.never_completes {
            return std::future::pending().await;
        }
        assert!(
            input
                .source_dir
                .as_ref()
                .unwrap()
                .join("paper.tex")
                .exists()
        );
        assert!(!work.exists() || work.read_dir().unwrap().next().is_none());
        let targets: Vec<&str> = input.targets.iter().map(|t| t.claim.as_str()).collect();
        assert_eq!(targets, ["C1"]);
        Ok(FormalOut {
            toolchain: "leanprover/lean4:v4.33.0, Mathlib v4.33.0".into(),
            files: vec![
                CheckedFile {
                    claim: "C1".into(),
                    theorem: Some("Wishpool.C1.main".into()),
                    lean: "import Mathlib".into(),
                    compiled: true,
                    axioms: vec!["propext".into()],
                    note: "Faithful".into(),
                    log: String::new(),
                },
                CheckedFile {
                    claim: "C2".into(),
                    theorem: None,
                    lean: String::new(),
                    compiled: true,
                    axioms: vec![],
                    note: "not a target".into(),
                    log: String::new(),
                },
            ],
            summary: "C1 formalized".into(),
        })
    }
}

#[async_trait]
impl Advisor for FakeAdvisor {
    fn engine(&self) -> &str {
        "test-advisor"
    }
    fn model(&self) -> &str {
        "codex-test"
    }
    async fn advise(&self, input: &AdvisorInput) -> ReviewResult<AdviceOut> {
        self.advice.fetch_add(1, Ordering::SeqCst);
        if self.fail_advice {
            return Err(ReviewError::Transport("advisor deadline".into()));
        }
        assert!(
            input
                .source_dir
                .as_ref()
                .unwrap()
                .join("paper.tex")
                .exists()
        );
        assert_eq!(input.main_file.as_deref(), Some("paper.tex"));
        assert!(!input.referee.text.is_empty());
        assert!(input.text.as_ref().unwrap().contains("compactness"));
        let formalization = if self.propose.load(Ordering::SeqCst) {
            ["C1", "C2"]
                .into_iter()
                .map(|claim| FormalizationOut {
                    claim: claim.into(),
                    feasibility: "ready".into(),
                    effort: "small".into(),
                    plan: "state and prove".into(),
                    ..Default::default()
                })
                .collect()
        } else {
            vec![]
        };
        Ok(AdviceOut {
            summary: "A useful argument".into(),
            formalization,
            ..Default::default()
        })
    }
    async fn draft_letter(
        &self,
        _: &AdvisorInput,
        advice: Option<&AdviceOut>,
        formal: Option<&serde_json::Value>,
    ) -> ReviewResult<LetterOut> {
        self.letters.fetch_add(1, Ordering::SeqCst);
        *self.formal_seen.lock().await = formal.cloned();
        if self.fail_letter {
            return Err(ReviewError::Output("unusable letter".into()));
        }
        if let Some(advice) = advice {
            assert!(advice.summary.starts_with("A useful argument"));
        }
        Ok(LetterOut {
            subject: "Feedback".into(),
            body: "A precise mathematical point.".into(),
            note: "An argument.".into(),
        })
    }
}

struct World {
    worker: Worker,
    paper: Submission,
    author: Caller,
    oracle: Arc<FakeOracle>,
    advisor: Arc<FakeAdvisor>,
    formalizer: Arc<FakeFormalizer>,
    _work: tempfile::TempDir,
}

impl World {
    async fn new(outcomes: Vec<ReviewResult<OracleStatus>>, pdf: bool) -> Self {
        let stores = Arc::new(MemoryStores::default());
        let app = App::new(
            stores.ports(Arc::new(SystemClock), Arc::new(LatexReader)),
            Policy::default(),
            BTreeSet::new(),
        );
        let author = app
            .ensure_service_account(&"author".into(), "Author", Role::Endorser)
            .await
            .unwrap();
        let reviewer = app
            .ensure_service_account(&"claude".into(), "Review engine", Role::Reviewer)
            .await
            .unwrap();
        let referee_account = app
            .ensure_service_account(&"wishpool:referee".into(), "Referee", Role::Reviewer)
            .await
            .unwrap();
        let paper = app
            .submit_paper(
                &author,
                NewPaper {
                    ai_disclosure: AiDisclosure {
                        level: AiUse::None,
                        statement: "No AI used in this paper.".into(),
                    },
                    authors: vec![],
                    msc: vec![],
                    doi: None,
                    open_to_contributors: false,
                },
                Upload {
                    filename: "paper.tex".into(),
                    bytes: SOURCE.as_bytes().to_vec(),
                },
            )
            .await
            .unwrap();
        if pdf {
            app.record_compilation(&reviewer, &paper.id, 1, Ok(b"%PDF".to_vec()))
                .await
                .unwrap();
        }
        let paper = app.submission(&author, &paper.id).await.unwrap();
        let claims = paper
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
        let paper = app
            .confirm_claims(&author, &paper.id, claims)
            .await
            .unwrap();
        while stores.take_job().await.is_some() {}
        let jobs = Arc::new(MemoryJobs::new(stores));
        let oracle = Arc::new(FakeOracle {
            submitted: AtomicUsize::new(0),
            references: Mutex::new(vec![]),
            outcomes: Mutex::new(outcomes.into()),
            submit_error: AtomicUsize::new(0),
        });
        let advisor = Arc::new(FakeAdvisor::default());
        let formalizer = Arc::new(FakeFormalizer::default());
        let work = tempfile::tempdir().unwrap();
        let worker = Worker {
            app,
            jobs,
            compile: Compile {
                tex_bin: "unused".into(),
                cache_dir: work.path().into(),
                texmf_home: None,
                timeout: Duration::from_secs(1),
            },
            model: None,
            openalex: Arc::new(
                wishpool_review::openalex::OpenAlex::new("http://127.0.0.1:1", None).unwrap(),
            ),
            reviewer,
            referee_account,
            referee_model: "chatgpt-pro".into(),
            oracle: Some(oracle.clone()),
            advisor: Some(advisor.clone()),
            formalizer: Some(formalizer.clone()),
            oracle_poll: Duration::ZERO,
            advisor_work_dir: work.path().into(),
        };
        Self {
            worker,
            paper,
            author,
            oracle,
            advisor,
            formalizer,
            _work: work,
        }
    }

    fn job(&self) -> LeasedJob {
        LeasedJob {
            submission: self.paper.id.clone(),
            kind: JobKind::Referee,
            attempts: 1,
            lease: "local".into(),
        }
    }

    async fn file(&self) -> wishpool_core::model::RefereeFile {
        self.worker
            .app
            .referee(&self.worker.referee_account, &self.paper.id)
            .await
            .unwrap()
    }

    async fn run_round(&self) {
        let mut job = self.job();
        for _ in 0..12 {
            match self.worker.referee_job(&job).await {
                Ok(false) => return,
                Ok(true) => {
                    job = self
                        .worker
                        .jobs
                        .claim()
                        .await
                        .unwrap()
                        .expect("deferred job")
                }
                Err(_) => panic!("worker failed"),
            }
        }
        panic!("round did not settle");
    }
}

fn completed(recommendation: &str) -> ReviewResult<OracleStatus> {
    Ok(OracleStatus::Completed { text: serde_json::json!({"recommendation":recommendation,"summary":"A review",
        "claims":[{"claim":"C1","shape":"content","witnesses":["A compactness lemma"],"known":"A reported source","note":"The intermediate argument"},
            {"claim":"C2","shape":"bind_only"}], "limits":["Computation was not reproduced"]}).to_string() })
}

#[tokio::test]
async fn polling_survives_transport_errors_without_resubmitting_or_burning_attempts() {
    let w = World::new(
        vec![
            Err(ReviewError::Transport("temporary".into())),
            Ok(OracleStatus::Queued { position: Some(1) }),
            Ok(OracleStatus::Running),
            completed("accept"),
        ],
        true,
    )
    .await;
    w.run_round().await;
    let file = w.file().await;
    let round = file.current().unwrap();
    assert!(round.is_settled());
    assert_eq!(round.referee.attempts, 1);
    assert_eq!(round.referee.input_digest.as_ref().unwrap().len(), 64);
    assert_eq!(
        round.referee.client_ref.as_deref(),
        Some(format!("wishpool:{}:r1:v1:c1", w.paper.id).as_str())
    );
    assert_eq!(
        round.referee.done().unwrap().recommendation,
        Some(Recommendation::Accept)
    );
    assert_eq!(w.oracle.submitted.load(Ordering::SeqCst), 1);
    assert_eq!(w.advisor.advice.load(Ordering::SeqCst), 1);
    assert_eq!(w.advisor.letters.load(Ordering::SeqCst), 1);
    assert!(round.advice.input_digest.is_some() && round.letter.input_digest.is_some());
    let judgements = w
        .worker
        .app
        .judgements(&w.worker.referee_account, &w.paper.id)
        .await
        .unwrap();
    assert_eq!(judgements.len(), 1);
    assert!(
        matches!(&judgements[0].reviewer, ReviewerIdentity::Machine { account, engine, model } if account.as_str() == "wishpool:referee" && engine == "nyxid-oracle" && model.as_deref() == Some("chatgpt-pro"))
    );
    assert!(judgements[0].rationale.contains("A reported source"));
    assert!(
        w.worker
            .app
            .referee(&w.author, &w.paper.id)
            .await
            .unwrap()
            .letters
            .is_empty()
    );
    assert!(!w.worker.referee_job(&w.job()).await.unwrap());
    assert_eq!(
        w.worker
            .app
            .judgements(&w.worker.referee_account, &w.paper.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn uncertain_submit_reuses_persisted_reference() {
    let w = World::new(vec![completed("minor_revision")], true).await;
    w.oracle.submit_error.store(1, Ordering::SeqCst);
    assert!(w.worker.referee_job(&w.job()).await.unwrap());
    let step = w.file().await.current().unwrap().referee.clone();
    assert!(matches!(step.state, StepState::Pending));
    assert!(step.client_ref.is_some() && step.input_digest.is_some());
    let job = w.worker.jobs.claim().await.unwrap().unwrap();
    assert_eq!(job.attempts, 1);
    assert!(w.worker.referee_job(&job).await.unwrap());
    w.run_round().await;
    let references = w.oracle.references.lock().await;
    assert_eq!(references.len(), 2);
    assert_eq!(references[0], references[1]);
}

#[tokio::test]
async fn positive_round_formalizes_proposed_proved_statements_before_the_letter() {
    let w = World::new(vec![completed("minor_revision")], true).await;
    w.advisor.propose.store(true, Ordering::SeqCst);
    w.run_round().await;
    let file = w.file().await;
    let round = file.current().unwrap();
    let probe = round.formal.done().expect("probe recorded");
    assert_eq!(w.formalizer.calls.load(Ordering::SeqCst), 1);
    assert_eq!(round.formal.engine.as_deref(), Some("test-lean"));
    assert_eq!(probe.attempts.len(), 1);
    assert_eq!(probe.attempts[0].claim.as_str(), "C1");
    assert_eq!(
        probe.attempts[0].outcome,
        wishpool_core::model::ProbeOutcome::Compiled
    );
    let seen = w
        .advisor
        .formal_seen
        .lock()
        .await
        .clone()
        .expect("letter saw the probe");
    assert_eq!(seen["attempts"][0]["outcome"], "compiled");
    assert!(round.letter.done().is_some());
}

#[tokio::test(start_paused = true)]
async fn formalization_budget_failure_defers_the_job_and_still_drafts_letter() {
    let mut w = World::new(vec![completed("accept")], true).await;
    w.advisor.propose.store(true, Ordering::SeqCst);
    let formalizer = Arc::new(FakeFormalizer {
        never_completes: true,
        ..Default::default()
    });
    w.worker.formalizer = Some(formalizer.clone());
    let mut job = w.job();
    // Submit, collect the report, and settle advice before the formal step.
    for _ in 0..3 {
        assert!(w.worker.referee_job(&job).await.unwrap());
        job = w.worker.jobs.claim().await.unwrap().expect("deferred job");
    }
    let started = tokio::time::Instant::now();
    assert!(w.worker.referee_job(&job).await.unwrap());
    // Independent of the production constant: the step must settle well
    // inside the 30-minute job lease.
    assert_eq!(started.elapsed(), Duration::from_secs(1500));
    assert!(started.elapsed() < Duration::from_secs(1800));
    let file = w.file().await;
    let round = file.current().unwrap();
    assert!(matches!(
        &round.formal.state,
        StepState::Failed {
            reason,
            detail: None,
            retryable: false,
            ..
        } if reason == "the formalization step exceeded its time budget"
    ));
    assert_eq!(round.formal.attempts, 1);
    assert!(matches!(round.letter.state, StepState::Pending));
    assert_eq!(w.advisor.letters.load(Ordering::SeqCst), 0);

    let deferred = w.worker.jobs.claim().await.unwrap().expect("deferred job");
    assert_eq!(deferred.submission, job.submission);
    assert_eq!(deferred.kind, job.kind);
    assert_eq!(deferred.attempts, job.attempts);
    assert_eq!(deferred.lease, job.lease);
    assert!(!w.worker.referee_job(&deferred).await.unwrap());
    let file = w.file().await;
    let round = file.current().unwrap();
    assert!(round.is_settled());
    assert!(round.letter.done().is_some());
    assert_eq!(w.advisor.letters.load(Ordering::SeqCst), 1);
    assert!(w.advisor.formal_seen.lock().await.is_none());
    assert!(!w.worker.referee_job(&deferred).await.unwrap());
    assert_eq!(formalizer.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn negative_recommendation_skips_advice_and_still_drafts_letter() {
    let w = World::new(vec![completed("major_revision")], true).await;
    w.run_round().await;
    let file = w.file().await;
    let round = file.current().unwrap();
    assert!(matches!(round.advice.state, StepState::Skipped { .. }));
    assert!(matches!(round.formal.state, StepState::Skipped { .. }));
    assert!(round.letter.done().is_some());
    assert_eq!(w.advisor.advice.load(Ordering::SeqCst), 0);
    assert_eq!(w.formalizer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(w.advisor.letters.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn confirmation_before_compilation_waits_without_submitting_or_burning_attempts() {
    let w = World::new(vec![completed("accept")], false).await;
    let mut job = w.job();
    for _ in 0..3 {
        assert!(w.worker.referee_job(&job).await.unwrap());
        let step = w.file().await.current().unwrap().referee.clone();
        assert!(matches!(step.state, StepState::Pending));
        assert_eq!(step.attempts, 0);
        assert!(step.client_ref.is_none() && step.input_digest.is_none());
        assert_eq!(w.oracle.submitted.load(Ordering::SeqCst), 0);
        job = w.worker.jobs.claim().await.unwrap().unwrap();
        assert_eq!(job.attempts, 1);
    }
    w.worker
        .app
        .record_compilation(&w.worker.reviewer, &w.paper.id, 1, Ok(b"%PDF".to_vec()))
        .await
        .unwrap();
    assert!(w.worker.referee_job(&job).await.unwrap());
    assert_eq!(w.oracle.submitted.load(Ordering::SeqCst), 1);
    assert!(matches!(
        w.file().await.current().unwrap().referee.state,
        StepState::Running { .. }
    ));
    w.run_round().await;
    assert_eq!(w.oracle.submitted.load(Ordering::SeqCst), 1);
    assert!(w.file().await.current().unwrap().is_settled());
}

#[tokio::test]
async fn failed_compilation_settles_without_oracle_or_advisor_work() {
    let w = World::new(vec![], false).await;
    w.worker
        .app
        .record_compilation(&w.worker.reviewer, &w.paper.id, 1, Err("TeX error".into()))
        .await
        .unwrap();
    w.run_round().await;
    let file = w.file().await;
    let round = file.current().unwrap();
    assert!(
        matches!(&round.referee.state, StepState::Failed { reason, retryable: false, .. } if reason == "the paper did not compile")
    );
    assert_eq!(round.referee.attempts, 0);
    assert!(round.is_settled());
    assert_eq!(w.oracle.submitted.load(Ordering::SeqCst), 0);
    assert_eq!(w.advisor.advice.load(Ordering::SeqCst), 0);
    assert_eq!(w.advisor.letters.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn remote_failure_and_missing_pdf_settle_without_advisor_work() {
    for status in [
        OracleStatus::Failed {
            reason: "prompt_delivery_uncertain".into(),
            detail: Some("page_crashed@waiting_response".into()),
        },
        OracleStatus::Cancelled,
    ] {
        let w = World::new(vec![Ok(status.clone())], true).await;
        w.run_round().await;
        let file = w.file().await;
        let round = file.current().unwrap();
        assert!(matches!(round.referee.state, StepState::Failed { .. }));
        if matches!(status, OracleStatus::Failed { .. }) {
            assert!(matches!(
                &round.referee.state,
                StepState::Failed {
                    retryable: true,
                    detail: Some(_),
                    ..
                }
            ));
        }
        assert!(matches!(round.advice.state, StepState::Skipped { .. }));
        assert!(matches!(round.letter.state, StepState::Skipped { .. }));
        assert_eq!(w.advisor.letters.load(Ordering::SeqCst), 0);
    }
    let w = World::new(vec![], true).await;
    // The PDF disappears after the submission snapshot was read.
    w.worker
        .app
        .record_compilation(&w.worker.reviewer, &w.paper.id, 1, Err("TeX error".into()))
        .await
        .unwrap();
    assert!(w.worker.resume_referee(&w.job(), &w.paper).await.unwrap());
    w.run_round().await;
    assert_eq!(w.oracle.submitted.load(Ordering::SeqCst), 0);
    assert!(
        matches!(&w.file().await.current().unwrap().referee.state, StepState::Failed { reason, retryable: false, .. } if reason == "the paper has no compiled PDF")
    );
}

#[tokio::test]
async fn disabled_oracle_completes_without_starting_round() {
    let mut w = World::new(vec![], true).await;
    w.worker.oracle = None;
    assert!(!w.worker.referee_job(&w.job()).await.unwrap());
    assert!(w.file().await.rounds.is_empty());
}

#[test]
fn digest_binds_boundaries_and_retryable_failure_reasons() {
    assert_ne!(input_digest("ab", b"c"), input_digest("a", b"bc"));
    for reason in [
        "infrastructure_retry_exhausted",
        "prompt_delivery_uncertain",
        "usage_limit_reached",
        "model_unavailable",
    ] {
        assert!(retryable(reason));
    }
    for reason in ["extraction_failure", "empty_response", "cancelled"] {
        assert!(!retryable(reason));
    }
}

#[tokio::test]
async fn existing_round_continues_after_editorial_decision() {
    let w = World::new(vec![completed("accept")], true).await;
    assert!(w.worker.referee_job(&w.job()).await.unwrap());
    let job = w.worker.jobs.claim().await.unwrap().unwrap();
    let stores = w.worker.app.clone();
    // The public policy flow is independent of the in-flight referee round;
    // use an ordinary editor's out-of-scope report to produce non-acceptance.
    let editor = stores
        .ensure_service_account(&"editor".into(), "Editor", Role::Editor)
        .await
        .unwrap();
    stores
        .file_report(
            &editor,
            &w.paper.id,
            wishpool_core::model::Stage::Literature,
            wishpool_core::model::ReportDraft {
                outcome: wishpool_core::model::Outcome::Fail {
                    reason: wishpool_core::model::RejectReason::OutOfScope {
                        detail: "Outside the venue's field".into(),
                    },
                },
                summary: "Outside scope".into(),
                payload: wishpool_core::model::StagePayload::Literature {
                    prior: vec![],
                    searched: vec!["Paper source: scope assessment".into()],
                },
                evidence: vec![],
            },
            None,
        )
        .await
        .unwrap();
    stores.decide(&editor, &w.paper.id).await.unwrap();
    assert!(
        !stores
            .submission(&editor, &w.paper.id)
            .await
            .unwrap()
            .is_open()
    );
    assert!(w.worker.referee_job(&job).await.unwrap());
    w.run_round().await;
    assert!(w.file().await.current().unwrap().is_settled());
    assert_eq!(w.advisor.letters.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn referee_reading_corroborates_claude_without_changing_policy() {
    let w = World::new(vec![completed("accept")], true).await;
    let output =
        mapping::judgement(true, &["A compactness lemma".into()], "Claude reading").unwrap();
    w.worker
        .app
        .file_machine_judgement(
            &w.worker.reviewer,
            &w.paper.id,
            &"C1".into(),
            output,
            FiledBy {
                engine: "openai-compatible".into(),
                model: Some("claude-test".into()),
            },
        )
        .await
        .unwrap();
    w.run_round().await;
    let analysis = w.worker.app.analysis(&w.author, &w.paper.id).await.unwrap();
    let reading = analysis
        .claims
        .iter()
        .find(|c| c.claim.as_str() == "C1")
        .unwrap();
    assert_eq!(
        reading.judgement.as_ref().unwrap().standing,
        wishpool_core::judgement::Standing::Corroborated
    );
    let paper = w
        .worker
        .app
        .submission(&w.author, &w.paper.id)
        .await
        .unwrap();
    assert!(paper.is_open());
    assert!(
        paper
            .latest_report(wishpool_core::model::Stage::Escape)
            .is_none()
    );
}

#[tokio::test]
async fn malformed_answer_is_retained_and_missing_advisor_is_explicit() {
    let mut w = World::new(
        vec![Ok(OracleStatus::Completed {
            text: "A readable but unstructured review".into(),
        })],
        true,
    )
    .await;
    w.worker.advisor = None;
    w.run_round().await;
    let file = w.file().await;
    let round = file.current().unwrap();
    let report = round.referee.done().unwrap();
    assert_eq!(report.text, "A readable but unstructured review");
    assert!(report.recommendation.is_none());
    assert!(!report.limits.is_empty());
    assert!(matches!(round.advice.state, StepState::Skipped { .. }));
    assert!(
        matches!(&round.letter.state, StepState::Failed { reason, retryable: false, .. } if reason == "no advisor configured")
    );
    let mut w = World::new(vec![completed("accept")], true).await;
    w.worker.advisor = None;
    w.run_round().await;
    assert!(
        matches!(&w.file().await.current().unwrap().advice.state, StepState::Failed { reason, .. } if reason == "no advisor configured")
    );
}

#[tokio::test]
async fn advisor_failure_still_allows_letter_and_invalid_letter_settles_failed() {
    for fail_advice in [true, false] {
        let mut w = World::new(vec![completed("accept")], true).await;
        let advisor = Arc::new(FakeAdvisor {
            fail_advice,
            fail_letter: !fail_advice,
            ..Default::default()
        });
        w.worker.advisor = Some(advisor.clone());
        w.run_round().await;
        let file = w.file().await;
        let round = file.current().unwrap();
        assert!(round.is_settled());
        if fail_advice {
            assert!(matches!(
                round.advice.state,
                StepState::Failed {
                    retryable: true,
                    ..
                }
            ));
            assert!(round.letter.done().is_some());
        } else {
            assert!(round.advice.done().is_some());
            assert!(matches!(
                round.letter.state,
                StepState::Failed {
                    retryable: false,
                    ..
                }
            ));
        }
        assert_eq!(advisor.advice.load(Ordering::SeqCst), 1);
        assert_eq!(advisor.letters.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn model_provenance_survives_config_changes_and_uncertain_admission() {
    let mut w = World::new(vec![completed("accept")], true).await;
    assert!(w.worker.referee_job(&w.job()).await.unwrap());
    w.worker.referee_model = "gpt-new-label".into();
    let job = w.worker.jobs.claim().await.unwrap().unwrap();
    assert!(w.worker.referee_job(&job).await.unwrap());
    let judgements = w
        .worker
        .app
        .judgements(&w.worker.referee_account, &w.paper.id)
        .await
        .unwrap();
    assert!(
        matches!(&judgements[0].reviewer, ReviewerIdentity::Machine { model, .. } if model.as_deref() == Some("chatgpt-pro"))
    );
    let mut w = World::new(vec![], true).await;
    w.oracle.submit_error.store(1, Ordering::SeqCst);
    assert!(w.worker.referee_job(&w.job()).await.unwrap());
    w.worker.referee_model = "gpt-new-label".into();
    w.run_round().await;
    assert_eq!(w.oracle.submitted.load(Ordering::SeqCst), 1);
    assert!(matches!(
        &w.file().await.current().unwrap().referee.state,
        StepState::Failed {
            retryable: false,
            ..
        }
    ));
}
