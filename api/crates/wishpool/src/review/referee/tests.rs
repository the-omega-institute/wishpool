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
        AiDisclosure, AiUse, Caller, ClaimConfirmation, FeedbackLetter, NewLetter, NewPaper,
        Recommendation, ReviewerIdentity, Role, StepState, Submission,
    },
    policy::Policy,
    ports::{JobKind, JobLease, SystemClock},
};
use wishpool_review::{
    ReviewResult,
    advisor::{Advisor, AdvisorInput},
    lean::{CheckedFile, FormalInput, FormalOut, Formalizer},
    oracle::{Oracle, OracleRequest, OracleStatus, OracleSubmitted},
    referee_prompts::{AdviceOut, AuditOut, AuditedClaimOut, FormalizationOut, LetterOut},
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
        if request.client_ref.starts_with("wishpool:problem:") {
            assert!(request.prompt.contains("C2") && request.pdf.is_none());
        } else {
            assert!(request.prompt.contains("C1") && request.prompt.contains("C2"));
            assert!(request.pdf.is_some());
        }
        assert!(request.prompt.contains("\"proved\": false"));
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
    audits: AtomicUsize,
    known: bool,
    audit_delay: Duration,
    audit_failures: AtomicUsize,
    reclaim_during_audit: Option<Arc<MemoryJobs>>,
}

#[derive(Default)]
struct FakeFormalizer {
    calls: AtomicUsize,
    never_completes: bool,
    corrections: Mutex<Vec<Option<String>>>,
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
        if input.conjecture {
            self.corrections.lock().await.push(input.correction.clone());
            let suffix = if input.correction.is_some() {
                "∀ n : Nat, n + 0 = n"
            } else {
                "∀ n : Nat, n = n"
            };
            return Ok(FormalOut {
                toolchain: "Lean fake checked workspace".into(),
                files: input
                    .targets
                    .iter()
                    .map(|t| CheckedFile {
                        claim: t.claim.clone(),
                        theorem: Some("wishpool_target_prop".into()),
                        lean: format!(
                            "import Mathlib\ndef wishpool_target_prop : Prop := {suffix}\n"
                        ),
                        compiled: true,
                        axioms: vec![],
                        note: "Every natural number equals itself.".into(),
                        log: String::new(),
                    })
                    .collect(),
                summary: "Statement elaborated".into(),
            });
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
    async fn audit(&self, input: &AdvisorInput) -> ReviewResult<AuditOut> {
        self.audits.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.audit_delay).await;
        if self
            .audit_failures
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(ReviewError::Transport("advisor CLI timed out".into()));
        }
        if let Some(jobs) = &self.reclaim_during_audit {
            // Model completion after a replacement worker obtained the lease.
            tokio::time::sleep(Duration::from_secs(31 * 60)).await;
            jobs.claim().await.unwrap().expect("replacement lease");
        }
        assert!(
            input
                .source_dir
                .as_ref()
                .unwrap()
                .join("paper.tex")
                .exists()
        );
        assert!(!input.referee.text.is_empty());
        Ok(AuditOut {
            verdict: "major_revision".into(),
            summary: "The argument is correct and carries new content.".into(),
            claims: input
                .statements
                .iter()
                .map(|c| AuditedClaimOut {
                    claim: c.id.clone(),
                    conjecture: (input.kind == "conjecture" && !c.proved).then(|| {
                        wishpool_review::referee_prompts::ConjectureOut {
                            well_posed: Some(true),
                            well_posed_reason: "Defined and quantified".into(),
                            status: "open".into(),
                            status_reason: "Open in available source/report".into(),
                            escape: "content".into(),
                            escape_reason: "Needs a new bound".into(),
                            ..Default::default()
                        }
                    }),
                    correctness: if c.proved { "correct" } else { "not_checked" }.into(),
                    comment: "The source supplies a compactness lemma.".into(),
                    shape: c.proved.then(|| "content".into()),
                    witnesses: if c.proved {
                        vec!["A compactness lemma".into()]
                    } else {
                        vec![]
                    },
                    known: (self.known && c.proved).then(|| {
                        "A reported source [opened: https://example.test/reported-work]".into()
                    }),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })
    }
    async fn advise(&self, input: &AdvisorInput) -> ReviewResult<AdviceOut> {
        assert!(input.audit.is_some() && input.decision.is_some());
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
        input: &AdvisorInput,
        advice: Option<&AdviceOut>,
    ) -> ReviewResult<LetterOut> {
        assert!(
            input.audit.is_some() && input.decision.as_ref().unwrap().get("decision").is_some()
        );
        self.letters.fetch_add(1, Ordering::SeqCst);
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
    jobs: Arc<MemoryJobs>,
    stores: Arc<MemoryStores>,
    paper: Submission,
    author: Caller,
    oracle: Arc<FakeOracle>,
    advisor: Arc<FakeAdvisor>,
    formalizer: Arc<FakeFormalizer>,
    _work: tempfile::TempDir,
}

impl World {
    async fn new(outcomes: Vec<ReviewResult<OracleStatus>>, pdf: bool) -> Self {
        Self::new_kind(outcomes, pdf, wishpool_core::model::SubmissionKind::Paper).await
    }
    async fn new_kind(
        outcomes: Vec<ReviewResult<OracleStatus>>,
        pdf: bool,
        kind: wishpool_core::model::SubmissionKind,
    ) -> Self {
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
                    kind,
                    make_public_after_acceptance: true,
                    typed_conjecture: None,
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
                depends_on_conjectures: vec![],
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
        let jobs = Arc::new(MemoryJobs::new(stores.clone()));
        let oracle = Arc::new(FakeOracle {
            submitted: AtomicUsize::new(0),
            references: Mutex::new(vec![]),
            outcomes: Mutex::new(outcomes.into()),
            submit_error: AtomicUsize::new(0),
        });
        let advisor = Arc::new(FakeAdvisor::default());
        let formalizer = Arc::new(FakeFormalizer::default());
        let work = tempfile::tempdir().unwrap();
        let auditor_account = app
            .ensure_service_account(&"wishpool:auditor".into(), "Auditor", Role::Reviewer)
            .await
            .unwrap();
        let worker = Worker {
            app,
            jobs: jobs.clone(),
            compile: Compile {
                tex_bin: "unused".into(),
                cache_dir: work.path().into(),
                texmf_home: None,
                timeout: Duration::from_secs(1),
            },
            model: None,
            reviewer,
            auditor_account,
            referee_account,
            referee_model: "chatgpt-pro".into(),
            oracle: Some(oracle.clone()),
            advisor: Some(advisor.clone()),
            formalizer: Some(formalizer.clone()),
            oracle_poll: Duration::ZERO,
            advisor_work_dir: work.path().into(),
            audit_budget: Duration::from_secs(3900),
            formal_budget: Duration::from_secs(1500),
        };
        Self {
            worker,
            jobs,
            stores,
            paper,
            author,
            oracle,
            advisor,
            formalizer,
            _work: work,
        }
    }

    async fn job(&self) -> LeasedJob {
        if let Some(job) = self.jobs.current(&self.paper.id, JobKind::Referee).await {
            return job;
        }
        wishpool_core::ports::ReviewQueue::enqueue(&*self.stores, &self.paper.id, JobKind::Referee)
            .await
            .unwrap();
        self.jobs.claim().await.unwrap().unwrap()
    }

    async fn claim_referee(&self) -> LeasedJob {
        loop {
            let job = self.jobs.claim().await.unwrap().expect("referee queued");
            if job.kind == JobKind::Referee {
                return job;
            }
            self.jobs
                .defer(&job, Duration::from_secs(24 * 3600))
                .await
                .unwrap();
        }
    }

    async fn file(&self) -> wishpool_core::model::RefereeFile {
        wishpool_core::ports::RefereeStore::get(&*self.stores, &self.paper.id)
            .await
            .unwrap()
            .unwrap_or_else(|| wishpool_core::model::RefereeFile::new(self.paper.id.clone()))
    }

    async fn run_round(&self) {
        let mut job = self.job().await;
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
    Ok(OracleStatus::Completed { model: None, text: serde_json::json!({"recommendation":recommendation,"summary":"A review",
        "claims":[{"claim":"C1","shape":"content","witnesses":["A compactness lemma"],"known":"A reported source [opened: https://example.test/reported-work]","note":"The intermediate argument"},
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
    assert!(
        judgements[0]
            .rationale
            .contains("A reported source [opened: https://example.test/reported-work]")
    );
    assert!(
        w.worker
            .app
            .referee(&w.author, &w.paper.id)
            .await
            .unwrap()
            .letters
            .len()
            == 1
    );
    assert!(!w.worker.referee_job(&w.job().await).await.unwrap());
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
    assert!(w.worker.referee_job(&w.job().await).await.unwrap());
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
async fn accepted_round_sends_letter_before_formalizing_proved_statements() {
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
    let StepState::Done { at: letter_at, .. } = round.letter.state else {
        panic!()
    };
    let StepState::Done { at: formal_at, .. } = round.formal.state else {
        panic!()
    };
    assert!(letter_at <= formal_at);
    assert_eq!(file.letters.len(), 1);
    assert_eq!(file.letters[0].sent_by.as_str(), "wishpool:auditor");
    assert_eq!(
        file.letters[0].assessment,
        Some(Recommendation::MajorRevision)
    );
    assert!(!file.letters[0].edited);
    assert!(round.letter.done().is_some());
}

#[tokio::test(start_paused = true)]
async fn formalization_budget_failure_follows_delivered_letter_and_defers_job() {
    let mut w = World::new(vec![completed("accept")], true).await;
    w.advisor.propose.store(true, Ordering::SeqCst);
    let formalizer = Arc::new(FakeFormalizer {
        never_completes: true,
        ..Default::default()
    });
    w.worker.formalizer = Some(formalizer.clone());
    w.worker.formal_budget = Duration::from_secs(3600 + 300);
    let mut job = w.job().await;
    // Submit, report, audit, decision + advice, then deliver letter.
    for _ in 0..5 {
        assert!(w.worker.referee_job(&job).await.unwrap());
        job = w.claim_referee().await;
    }
    let started = tokio::time::Instant::now();
    w.worker.handle(job.clone()).await;
    // A one-hour Codex deadline plus discovery/check margin runs beyond the
    // original lease, which the heartbeat must keep alive until deferral.
    assert_eq!(started.elapsed(), Duration::from_secs(3900));
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
    assert!(round.letter.done().is_some());
    assert_eq!(w.advisor.letters.load(Ordering::SeqCst), 1);

    let deferred = w.claim_referee().await;
    assert_eq!(deferred.submission, job.submission);
    assert_eq!(deferred.kind, job.kind);
    assert_eq!(deferred.attempts, job.attempts);
    assert_ne!(deferred.lease, job.lease);
    assert!(!w.worker.referee_job(&deferred).await.unwrap());
    let file = w.file().await;
    let round = file.current().unwrap();
    assert!(round.is_settled());
    assert!(round.letter.done().is_some());
    assert_eq!(w.advisor.letters.load(Ordering::SeqCst), 1);
    assert!(!w.worker.referee_job(&deferred).await.unwrap());
    assert_eq!(formalizer.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn negative_recommendation_still_gets_decision_advice_and_letter() {
    let w = World::new(vec![completed("major_revision")], true).await;
    w.run_round().await;
    let file = w.file().await;
    let round = file.current().unwrap();
    assert!(round.advice.done().is_some());
    assert!(matches!(round.formal.state, StepState::Skipped { .. }));
    assert!(round.letter.done().is_some());
    assert_eq!(w.advisor.advice.load(Ordering::SeqCst), 1);
    assert_eq!(w.formalizer.calls.load(Ordering::SeqCst), 0);
    assert_eq!(w.advisor.letters.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn confirmation_before_compilation_waits_without_submitting_or_burning_attempts() {
    let w = World::new(vec![completed("accept")], false).await;
    let mut job = w.job().await;
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
    assert!(
        w.worker
            .resume_referee(&w.job().await, &w.paper)
            .await
            .unwrap()
    );
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
    assert!(!w.worker.referee_job(&w.job().await).await.unwrap());
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
    assert!(w.worker.referee_job(&w.job().await).await.unwrap());
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
    assert!(matches!(paper.status, SubmissionStatus::Accepted { .. }));
    assert!(
        paper
            .latest_report(wishpool_core::model::Stage::Escape)
            .is_some()
    );
}

#[tokio::test]
async fn malformed_answer_is_retained_and_missing_advisor_is_explicit() {
    let mut w = World::new(
        vec![Ok(OracleStatus::Completed {
            text: "A readable but unstructured review".into(),
            model: None,
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
        matches!(&round.audit.state, StepState::Failed { reason, .. } if reason == "no Codex auditor configured")
    );
    let mut w = World::new(vec![completed("accept")], true).await;
    w.worker.advisor = None;
    w.run_round().await;
    assert!(matches!(
        &w.file().await.current().unwrap().audit.state,
        StepState::Failed { reason, .. } if reason == "no Codex auditor configured"
    ));
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
async fn observed_oracle_model_is_recorded_on_the_step_and_judgements() {
    let OracleStatus::Completed { text, .. } = completed("accept").unwrap() else {
        unreachable!();
    };
    let mut w = World::new(
        vec![Ok(OracleStatus::Completed {
            text,
            model: Some("gpt_6 · pro".into()),
        })],
        true,
    )
    .await;
    assert!(w.worker.referee_job(&w.job().await).await.unwrap());
    w.worker.referee_model = "gpt-new-label".into();
    w.run_round().await;
    assert_eq!(
        w.file().await.current().unwrap().referee.model.as_deref(),
        Some("gpt_6 · pro")
    );
    let judgements = w
        .worker
        .app
        .judgements(&w.worker.referee_account, &w.paper.id)
        .await
        .unwrap();
    assert_eq!(judgements.len(), 1);
    assert!(
        matches!(&judgements[0].reviewer, ReviewerIdentity::Machine { model, .. } if model.as_deref() == Some("gpt_6 · pro"))
    );
}

#[tokio::test]
async fn model_provenance_survives_config_changes_and_uncertain_admission() {
    let mut w = World::new(vec![completed("accept")], true).await;
    assert!(w.worker.referee_job(&w.job().await).await.unwrap());
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
    assert!(w.worker.referee_job(&w.job().await).await.unwrap());
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

#[tokio::test]
async fn sent_assessment_round_trips_without_becoming_a_publication_decision() {
    let w = World::new(vec![completed("accept")], true).await;
    w.run_round().await;
    let app = &w.worker.app;
    let editor = app
        .ensure_service_account(&"editor".into(), "Editor", Role::Editor)
        .await
        .unwrap();
    let before = app.submission(&w.author, &w.paper.id).await.unwrap();
    let decision = app.preview_decision(&editor, &w.paper.id).await.unwrap();
    let draft = w
        .file()
        .await
        .current()
        .unwrap()
        .letter
        .done()
        .unwrap()
        .clone();
    let mut request = serde_json::json!({
        "subject": draft.subject, "body": draft.body, "note": draft.note
    });
    let legacy: NewLetter = serde_json::from_value(request.clone()).unwrap();
    assert_eq!(legacy.assessment, None);
    let legacy = app
        .send_feedback(&editor, &w.paper.id, legacy)
        .await
        .unwrap();
    let legacy_json = serde_json::to_value(&legacy).unwrap();
    assert!(legacy_json.get("assessment").is_none());
    assert_eq!(
        serde_json::from_value::<FeedbackLetter>(legacy_json).unwrap(),
        legacy
    );

    // The editor can choose an assessment different from the referee's advice.
    request["assessment"] = serde_json::json!("major_revision");
    let letter = app
        .send_feedback(
            &editor,
            &w.paper.id,
            serde_json::from_value(request).unwrap(),
        )
        .await
        .unwrap();
    let serialized = serde_json::to_value(&letter).unwrap();
    assert_eq!(serialized["assessment"], "major_revision");
    assert_eq!(
        serde_json::from_value::<FeedbackLetter>(serialized).unwrap(),
        letter
    );
    let view = app.referee(&w.author, &w.paper.id).await.unwrap();
    assert_eq!(view.rounds.len(), 1);
    assert_eq!(&view.letters[1..], &[legacy, letter]);
    assert_eq!(
        app.submission(&w.author, &w.paper.id).await.unwrap(),
        before
    );
    assert_eq!(
        app.preview_decision(&editor, &w.paper.id).await.unwrap(),
        decision
    );
}

#[tokio::test]
async fn known_main_result_gets_advice_and_auto_letter_but_no_formal_probe() {
    let mut w = World::new(vec![completed("accept")], true).await;
    let advisor = Arc::new(FakeAdvisor {
        known: true,
        ..Default::default()
    });
    advisor.propose.store(true, Ordering::SeqCst);
    w.worker.advisor = Some(advisor.clone());
    w.run_round().await;
    let paper = w
        .worker
        .app
        .submission(&w.author, &w.paper.id)
        .await
        .unwrap();
    assert_eq!(paper.status, SubmissionStatus::NotAccepted);
    assert_eq!(advisor.advice.load(Ordering::SeqCst), 1);
    assert_eq!(advisor.letters.load(Ordering::SeqCst), 1);
    assert_eq!(w.formalizer.calls.load(Ordering::SeqCst), 0);
    let file = w.file().await;
    assert!(
        file.letters[0]
            .body
            .starts_with("Your paper is not accepted.")
    );
    assert!(
        file.letters[0]
            .body
            .contains("A reported source [opened: https://example.test/reported-work]")
    );
    assert!(matches!(
        file.current().unwrap().formal.state,
        StepState::Skipped { .. }
    ));
    assert_eq!(
        file.letters[0].assessment,
        Some(Recommendation::MajorRevision)
    );
}

async fn prepare_audit(w: &World) -> LeasedJob {
    let mut job = w.job().await;
    for _ in 0..2 {
        assert!(w.worker.referee_job(&job).await.unwrap());
        job = w.worker.jobs.claim().await.unwrap().unwrap();
    }
    assert!(w.file().await.current().unwrap().referee.done().is_some());
    job
}

#[tokio::test(start_paused = true)]
async fn heartbeat_allows_audit_to_finish_past_the_original_lease() {
    let mut w = World::new(vec![completed("accept")], true).await;
    let advisor = Arc::new(FakeAdvisor {
        audit_delay: Duration::from_secs(41 * 60),
        ..Default::default()
    });
    w.worker.advisor = Some(advisor.clone());
    let job = prepare_audit(&w).await;
    let started = tokio::time::Instant::now();
    w.worker.handle(job.clone()).await;
    assert_eq!(started.elapsed(), Duration::from_secs(41 * 60));
    let file = w.file().await;
    let round = file.current().unwrap();
    assert!(round.audit.done().is_some());
    assert_eq!(round.audit.attempts, 1);
    assert!(matches!(round.advice.state, StepState::Pending));
    let next = w.worker.jobs.claim().await.unwrap().unwrap();
    assert_eq!(next.attempts, job.attempts);
    assert_ne!(next.lease, job.lease);
    assert_eq!(advisor.audits.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn audit_transport_timeout_retries_only_audit_with_existing_accounting() {
    let mut w = World::new(vec![completed("accept")], true).await;
    let advisor = Arc::new(FakeAdvisor {
        audit_failures: AtomicUsize::new(1),
        ..Default::default()
    });
    w.worker.advisor = Some(advisor.clone());
    let job = prepare_audit(&w).await;
    w.worker.handle(job).await;
    let file = w.file().await;
    let round = file.current().unwrap();
    assert!(matches!(round.audit.state, StepState::Running { .. }));
    assert_eq!(round.audit.attempts, 1);
    assert!(matches!(round.advice.state, StepState::Pending));
    assert!(matches!(round.letter.state, StepState::Pending));
    assert!(matches!(round.formal.state, StepState::Pending));
    assert_eq!(advisor.advice.load(Ordering::SeqCst), 0);
    assert!(w.worker.jobs.claim().await.unwrap().is_none());
    tokio::time::advance(Duration::from_secs(30)).await;
    let retry = w.worker.jobs.claim().await.unwrap().unwrap();
    assert_eq!(retry.attempts, 2);
    w.worker.handle(retry).await;
    let file = w.file().await;
    assert!(file.current().unwrap().audit.done().is_some());
    assert_eq!(file.current().unwrap().audit.attempts, 2);
    assert_eq!(file.current().unwrap().referee.attempts, 1);
    assert_eq!(w.oracle.submitted.load(Ordering::SeqCst), 1);
    assert_eq!(advisor.audits.load(Ordering::SeqCst), 2);
    assert_eq!(advisor.advice.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn stale_audit_result_cannot_overwrite_the_replacement_workers_step() {
    let mut w = World::new(vec![completed("accept")], true).await;
    let job = prepare_audit(&w).await;
    w.worker.advisor = Some(Arc::new(FakeAdvisor {
        reclaim_during_audit: Some(w.jobs.clone()),
        ..Default::default()
    }));
    assert!(matches!(
        w.worker.referee_job(&job).await,
        Err(Failure::LeaseLost)
    ));
    assert!(!w.jobs.renew(&job).await.unwrap());
    let replacement = w.jobs.current(&w.paper.id, JobKind::Referee).await.unwrap();
    assert_ne!(replacement.lease, job.lease);
    let before = w.file().await;
    assert!(matches!(
        before.current().unwrap().audit.state,
        StepState::Running { .. }
    ));
    w.worker.advisor = Some(w.advisor.clone());
    assert!(w.worker.referee_job(&replacement).await.unwrap());
    let after = w.file().await;
    assert!(after.current().unwrap().audit.done().is_some());
    let revision = after.revision;
    assert!(matches!(
        w.worker.referee_job(&job).await,
        Err(Failure::LeaseLost)
    ));
    assert_eq!(w.file().await.revision, revision);
}

struct LostHeartbeat {
    jobs: Arc<MemoryJobs>,
    lose_at: tokio::time::Instant,
    renewals: AtomicUsize,
    settlements: AtomicUsize,
}

#[async_trait]
impl JobLease for LostHeartbeat {
    async fn claim(&self) -> wishpool_core::CoreResult<Option<LeasedJob>> {
        self.jobs.claim().await
    }
    async fn renew(&self, job: &LeasedJob) -> wishpool_core::CoreResult<bool> {
        self.renewals.fetch_add(1, Ordering::SeqCst);
        if tokio::time::Instant::now() >= self.lose_at {
            Ok(false)
        } else {
            self.jobs.renew(job).await
        }
    }
    async fn complete(&self, _job: &LeasedJob) -> wishpool_core::CoreResult<()> {
        self.settlements.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn defer(&self, _job: &LeasedJob, _delay: Duration) -> wishpool_core::CoreResult<()> {
        self.settlements.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn retry(&self, _job: &LeasedJob, _error: &str) -> wishpool_core::CoreResult<()> {
        self.settlements.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
async fn lost_heartbeat_cancels_audit_and_never_settles_the_job() {
    let mut w = World::new(vec![completed("accept")], true).await;
    let job = prepare_audit(&w).await;
    w.worker.advisor = Some(Arc::new(FakeAdvisor {
        audit_delay: Duration::from_secs(41 * 60),
        ..Default::default()
    }));
    let jobs = Arc::new(LostHeartbeat {
        jobs: w.jobs.clone(),
        lose_at: tokio::time::Instant::now() + Duration::from_secs(60),
        renewals: AtomicUsize::new(0),
        settlements: AtomicUsize::new(0),
    });
    w.worker.jobs = jobs.clone();
    let started = tokio::time::Instant::now();
    w.worker.handle(job).await;
    assert_eq!(started.elapsed(), Duration::from_secs(5 * 60));
    let file = w.file().await;
    assert!(matches!(
        file.current().unwrap().audit.state,
        StepState::Running { .. }
    ));
    assert!(matches!(
        file.current().unwrap().advice.state,
        StepState::Pending
    ));
    assert_eq!(jobs.settlements.load(Ordering::SeqCst), 0);
    let renewals = jobs.renewals.load(Ordering::SeqCst);
    tokio::time::advance(Duration::from_secs(3600)).await;
    assert_eq!(
        jobs.renewals.load(Ordering::SeqCst),
        renewals,
        "heartbeat ended with the job"
    );
}

#[tokio::test]
async fn conjecture_pipeline_displays_before_letter_then_generates_and_regenerates_target() {
    use wishpool_core::model::{
        AuthenticationMethod, LeanStatementResponse, SubmissionKind, SubmissionStatus,
    };
    let answer = serde_json::json!({"recommendation":"accept","summary":"Open conjecture", "claims":[{"claim":"C2","conjecture":{
        "well_posed":true,"well_posed_reason":"Defined symbols", "status":"open","status_reason":"Open as far as available material establishes", "named_works":[],
        "escape":"content","escape_reason":"Needs a new bound","suggestions":["Give the boundary case"]}}]}).to_string();
    let w = World::new_kind(
        vec![Ok(OracleStatus::Completed {
            text: answer,
            model: None,
        })],
        true,
        SubmissionKind::Conjecture,
    )
    .await;
    let mut job = w.job().await;
    // Admission, report, audit; publication is applied in the next call before advice/letter.
    for _ in 0..4 {
        assert!(w.worker.referee_job(&job).await.unwrap());
        job = w.jobs.claim().await.unwrap().unwrap();
    }
    let current = w
        .worker
        .app
        .submission(&w.author, &w.paper.id)
        .await
        .unwrap();
    let record = match current.status {
        SubmissionStatus::Accepted { record } => record,
        _ => panic!("not displayed"),
    };
    assert_eq!(
        w.worker
            .app
            .list_conjectures(None, None)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(w.file().await.letters.is_empty());
    assert!(w.worker.app.paper(&record).await.unwrap().claims.len() == 2);
    // Letter and target scheduling.
    assert!(w.worker.referee_job(&job).await.unwrap());
    job = w.jobs.claim().await.unwrap().unwrap();
    assert!(
        w.file().await.letters[0]
            .body
            .contains("Your conjecture is displayed")
    );
    assert!(w.worker.referee_job(&job).await.unwrap());
    // Claim whichever queued job is next, process both safely.
    for _ in 0..3 {
        let Some(next) = w.jobs.claim().await.unwrap() else {
            break;
        };
        w.worker.handle(next).await;
    }
    let current = w
        .worker
        .app
        .submission(&w.author, &w.paper.id)
        .await
        .unwrap();
    assert_eq!(current.lean_statements.len(), 1);
    assert!(matches!(
        current.lean_statements[0].response,
        LeanStatementResponse::AwaitingAuthor
    ));
    assert!(
        w.worker
            .app
            .paper(&record)
            .await
            .unwrap()
            .lean_statements
            .is_empty()
    );
    let digest = &current.lean_statements[0].digest;
    w.worker
        .app
        .respond_lean_statement(
            &w.author,
            AuthenticationMethod::CookieSession,
            &w.paper.id,
            digest,
            false,
            "Keep the zero term explicit".into(),
        )
        .await
        .unwrap();
    let target_job = w.jobs.claim().await.unwrap().unwrap();
    assert_eq!(target_job.kind, JobKind::LeanStatement);
    w.worker.handle(target_job).await;
    let current = w
        .worker
        .app
        .submission(&w.author, &w.paper.id)
        .await
        .unwrap();
    assert_eq!(current.lean_statements.len(), 2);
    assert_ne!(
        current.lean_statements[0].digest,
        current.lean_statements[1].digest
    );
    assert_eq!(
        w.formalizer.corrections.lock().await.as_slice(),
        [None, Some("Keep the zero term explicit".into())]
    );
}

#[tokio::test]
async fn accepted_paper_problem_runs_independent_referee_audit_and_author_target_confirmation() {
    use wishpool_core::{
        model::{AuthenticationMethod, SubmissionStatus},
        ports::ReviewQueue,
    };
    let problem = serde_json::json!({"recommendation":"accept","summary":"Open question in the paper", "claims":[{"claim":"C2","conjecture":{"well_posed":true,"well_posed_reason":"Defined symbols","status":"open","status_reason":"Not settled in the paper","escape":"content","escape_reason":"Needs a new bound"}}]}).to_string();
    let w = World::new(
        vec![
            completed("accept"),
            Ok(OracleStatus::Completed {
                text: problem,
                model: None,
            }),
        ],
        true,
    )
    .await;
    w.run_round().await;
    for kind in [
        JobKind::Referee,
        JobKind::OpenProblems,
        JobKind::LeanStatement,
    ] {
        if let Some(job) = w.jobs.current(&w.paper.id, kind).await {
            w.jobs.complete(&job).await.unwrap();
        }
    }
    // Keep this test focused on the independently leased candidate job.
    while let Some(job) = w.jobs.claim().await.unwrap() {
        w.jobs.complete(&job).await.unwrap();
    }
    let paper = w
        .worker
        .app
        .submission(&w.author, &w.paper.id)
        .await
        .unwrap();
    let SubmissionStatus::Accepted { record } = paper.status else {
        panic!()
    };
    assert!(
        w.worker
            .app
            .list_conjectures(None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    ReviewQueue::enqueue(&*w.stores, &w.paper.id, JobKind::OpenProblems)
        .await
        .unwrap();
    let mut job = w.jobs.claim().await.unwrap().unwrap();
    assert_eq!(job.kind, JobKind::OpenProblems);
    for _ in 0..2 {
        assert!(w.worker.open_problems_job(&job).await.unwrap());
        job = w.jobs.claim().await.unwrap().unwrap();
    }
    assert!(!w.worker.open_problems_job(&job).await.unwrap());
    w.jobs.complete(&job).await.unwrap();
    let files = w
        .worker
        .app
        .open_problem_files(&w.worker.reviewer, &w.paper.id)
        .await
        .unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].admitted && files[0].audit.is_some());
    let listed = w.worker.app.list_conjectures(None, None).await.unwrap();
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].claim.as_str(), "C2");
    assert!(listed.items[0].source.starts_with("from WP-"));
    // Staff-triggered candidates also work for old accepted papers with no
    // delivered primary letter; their own independent report supplies context.
    let mut primary = wishpool_core::ports::RefereeStore::get(&*w.stores, &w.paper.id)
        .await
        .unwrap()
        .unwrap();
    let expected = primary.revision;
    primary.rounds.clear();
    primary.revision += 1;
    wishpool_core::ports::RefereeStore::replace(&*w.stores, &primary, expected)
        .await
        .unwrap();
    let target_job = w.jobs.claim().await.unwrap().unwrap();
    assert_eq!(target_job.kind, JobKind::LeanStatement);
    w.worker.handle(target_job).await;
    let current = w
        .worker
        .app
        .submission(&w.author, &w.paper.id)
        .await
        .unwrap();
    assert_eq!(current.lean_statements.len(), 1);
    assert!(w.worker.app.target(&record, &"C2".into()).await.is_err());
    w.worker
        .app
        .respond_lean_statement(
            &w.author,
            AuthenticationMethod::CookieSession,
            &w.paper.id,
            &current.lean_statements[0].digest,
            true,
            String::new(),
        )
        .await
        .unwrap();
    assert!(
        w.worker
            .app
            .target(&record, &"C2".into())
            .await
            .unwrap()
            .lean
            .contains("def wishpool_target_prop")
    );
    let before = w.oracle.submitted.load(Ordering::SeqCst);
    ReviewQueue::enqueue(&*w.stores, &w.paper.id, JobKind::OpenProblems)
        .await
        .unwrap();
    let job = w.jobs.claim().await.unwrap().unwrap();
    assert!(!w.worker.open_problems_job(&job).await.unwrap());
    assert_eq!(w.oracle.submitted.load(Ordering::SeqCst), before);
}

struct ManagedTransport {
    created: AtomicUsize,
    calls: Mutex<Vec<(String, String)>>,
    answers: Mutex<std::collections::BTreeMap<String, String>>,
}
#[async_trait]
impl wishpool_review::cma::Transport for ManagedTransport {
    async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
        _key: Option<&str>,
    ) -> ReviewResult<wishpool_review::cma::TransportReply> {
        use serde_json::json;
        self.calls.lock().await.push((method.into(), path.into()));
        let mut response_id = None;
        let value = if path.ends_with("/agents") {
            let n = self.created.fetch_add(1, Ordering::SeqCst) + 1;
            json!({"id":format!("agt_{n}")})
        } else if method == "POST" && path.ends_with("/responses") {
            let body = body.unwrap();
            assert_eq!(body["stream"], true);
            let text = body["input"].as_str().unwrap();
            assert!(text.contains("Every gap is finite"));
            assert!(!text.contains("Network access is disabled"));
            let answer = if text.contains("Audit the GPT Pro referee report") {
                json!({"verdict":"accept","summary":"The compactness argument supplies new content.","claims":[{"claim":"C1","correctness":"correct","comment":"A compactness lemma supplies the proof.","shape":"content","witnesses":["A compactness lemma"],"referee_agreed":true},{"claim":"C2","correctness":"not_checked","comment":"An open question."}]})
            } else if text.contains("You are advising") {
                json!({"summary":"Clarify the notation.","improvements":[],"formalization":[]})
            } else {
                json!({"subject":"Decision","body":"Dear A. Author,\n\nThe compactness argument is useful.","note":""})
            };
            let id = format!("resp_{}", self.created.load(Ordering::SeqCst));
            self.answers
                .lock()
                .await
                .insert(id.clone(), answer.to_string());
            response_id = Some(id);
            json!(null)
        } else if method == "GET" && path.starts_with("api/v2/responses/") {
            let id = path.rsplit('/').next().unwrap();
            let answer = self.answers.lock().await.get(id).unwrap().clone();
            json!({"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":answer}]}]})
        } else if path.ends_with("/controls") {
            json!({"target_active":false,"target":{}})
        } else {
            json!(null)
        };
        Ok(wishpool_review::cma::TransportReply {
            status: 200,
            body: if value.is_null() {
                String::new()
            } else {
                value.to_string()
            },
            response_id,
        })
    }
}
#[tokio::test]
async fn pipeline_uses_durable_cma_for_audit_advice_letter_and_hides_provider_state() {
    let mut w = World::new(vec![completed("accept")], true).await;
    let transport = Arc::new(ManagedTransport {
        created: AtomicUsize::new(0),
        calls: Mutex::new(vec![]),
        answers: Mutex::new(Default::default()),
    });
    w.worker.advisor = Some(Arc::new(wishpool_review::cma::CmaAdvisor {
        client: Arc::new(wishpool_review::cma::Client {
            transport: transport.clone(),
            workspace: "wks_test".into(),
            profile: None,
            poll: Duration::from_millis(1),
        }),
        timeout: Duration::from_secs(60),
        audit_timeout: Duration::from_secs(60),
    }));
    w.run_round().await;
    let file = w.file().await;
    assert_eq!(file.agent_steps.len(), 3);
    assert_eq!(
        file.current().unwrap().audit.engine.as_deref(),
        Some("nyxid-cma")
    );
    assert_eq!(file.letters.len(), 1);
    for step in file.agent_steps.values() {
        assert_eq!(step.progress["stopped"], true);
        assert_eq!(step.progress["deleted"], true);
        assert!(
            step.progress["agent_id"]
                .as_str()
                .unwrap()
                .starts_with("agt_")
        );
        assert!(
            step.progress["response_id"]
                .as_str()
                .unwrap()
                .starts_with("resp_")
        );
    }
    let view = serde_json::to_string(&w.worker.app.referee(&w.author, &w.paper.id).await.unwrap())
        .unwrap();
    assert!(!view.contains("agent_steps") && !view.contains("agt_") && !view.contains("resp_"));
    assert_eq!(transport.created.load(Ordering::SeqCst), 3);
}
#[tokio::test]
async fn withdrawn_paper_cleans_up_its_saved_cma_turn_after_a_worker_restart() {
    use wishpool_review::cma::{RunState, RunStore};
    let mut w = World::new(vec![], true).await;
    let transport = Arc::new(ManagedTransport {
        created: AtomicUsize::new(0),
        calls: Mutex::new(vec![]),
        answers: Mutex::new(Default::default()),
    });
    w.worker.advisor = Some(Arc::new(wishpool_review::cma::CmaAdvisor {
        client: Arc::new(wishpool_review::cma::Client {
            transport: transport.clone(),
            workspace: "wks_test".into(),
            profile: None,
            poll: Duration::from_millis(1),
        }),
        timeout: Duration::from_secs(60),
        audit_timeout: Duration::from_secs(60),
    }));
    let job = w.job().await;
    let store = w.worker.agent_store(&job, &w.paper, "r1:audit:a0");
    store
        .save(
            serde_json::to_value(RunState {
                agent_id: Some("agt_previous_worker".into()),
                response_id: Some("resp_previous_worker".into()),
                deadline: u64::MAX,
                ..Default::default()
            })
            .unwrap(),
        )
        .await
        .unwrap();
    w.worker.app.withdraw(&w.author, &w.paper.id).await.unwrap();
    w.worker.cleanup_agents(&job).await.unwrap();
    let state = store.load().await.unwrap().unwrap();
    assert_eq!(state["deleted"], true);
    assert!(
        transport
            .calls
            .lock()
            .await
            .iter()
            .any(|(method, path)| method == "DELETE" && path.ends_with("agt_previous_worker"))
    );
    assert_eq!(transport.created.load(Ordering::SeqCst), 0);
}
