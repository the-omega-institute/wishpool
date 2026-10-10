//! MongoDB integration tests. They run when `WISHPOOL_TEST_MONGODB_URI` names
//! a disposable server and are skipped otherwise; CI provides a MongoDB
//! service. Each test uses its own database and drops it afterwards.

use std::{collections::BTreeSet, sync::Arc};

use chrono::Utc;
use wishpool_core::{
    CoreError,
    app::{App, Upload},
    ids::{ClaimId, PersonId, RecordId, SubmissionId},
    model::*,
    policy::Policy,
    ports::*,
};

use super::*;
use crate::{
    auth::{AuthStore, LoginAttempt, SessionRecord},
    latex::LatexReader,
    review::JobLease,
};

struct TestDb {
    store: MongoStore,
    client: Client,
    name: String,
}

impl TestDb {
    async fn open() -> Option<Self> {
        let Ok(uri) = std::env::var("WISHPOOL_TEST_MONGODB_URI") else {
            eprintln!("WISHPOOL_TEST_MONGODB_URI not set; skipping MongoDB test");
            return None;
        };
        let name = format!("wishpool_test_{}", uuid_like());
        let store = MongoStore::connect(&uri, &name)
            .await
            .expect("connect to test MongoDB");
        let client = Client::with_uri_str(&uri).await.unwrap();
        Some(Self {
            store,
            client,
            name,
        })
    }

    async fn drop(self) {
        self.client.database(&self.name).drop().await.unwrap();
    }
}

fn uuid_like() -> String {
    crate::auth::random_token()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(16)
        .collect()
}

fn submission(id: &str, submitter: &str, status: SubmissionStatus) -> Submission {
    Submission {
        problem_check_requested: false,
        conjecture_dependencies: vec![],
        kind: wishpool_core::model::SubmissionKind::Paper,
        lean_statements: vec![],
        id: SubmissionId(id.into()),
        submitter: PersonId(submitter.into()),
        title: "t".into(),
        abstract_text: String::new(),
        authors: vec![],
        ai_disclosure: AiDisclosure {
            level: AiUse::None,
            statement: "none".into(),
        },
        msc: vec![],
        doi: None,
        versions: vec![],
        extracted: vec![],
        claims: vec![],
        claims_revision: 0,
        reports: vec![],
        status,
        decision: None,
        open_to_contributors: false,
        analysis_visibility: Visibility::Undecided,
        formalization: FormalizationPlan::default(),
        conjectures: vec![],
        published_progress: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        revision: 0,
    }
}

#[tokio::test]
async fn people_submissions_and_revision_fencing() {
    let Some(db) = TestDb::open().await else {
        return;
    };
    let s = &db.store;
    let identity = VerifiedIdentity {
        subject: "u1".into(),
        name: Some("Ada".into()),
        email: Some("a@x.org".into()),
        picture: None,
    };
    let person = s.upsert_sign_in(&identity, Utc::now()).await.unwrap();
    assert_eq!(person.display_name, "Ada");
    let person = s
        .set_roles(&person.id, &[Role::Editor, Role::Editor, Role::Admin])
        .await
        .unwrap();
    assert_eq!(person.roles, BTreeSet::from([Role::Editor, Role::Admin]));
    // Signing in again refreshes the profile and keeps roles.
    let again = s
        .upsert_sign_in(
            &VerifiedIdentity {
                email: None,
                ..identity
            },
            Utc::now(),
        )
        .await
        .unwrap();
    assert_eq!(again.roles.len(), 2);
    assert_eq!(again.email, None);

    let statuses = [
        SubmissionStatus::Draft,
        SubmissionStatus::InReview,
        SubmissionStatus::NotAccepted,
        SubmissionStatus::Withdrawn,
        SubmissionStatus::InReview,
    ];
    for (i, status) in statuses.into_iter().enumerate() {
        SubmissionStore::insert(s, &submission(&format!("s{i}"), "u1", status))
            .await
            .unwrap();
    }
    assert_eq!(s.active_count(&"u1".into()).await.unwrap(), 3);
    let mine = SubmissionFilter {
        involving: Some("u1".into()),
        status: None,
    };
    let first = SubmissionStore::list(s, &mine, 2, None).await.unwrap();
    assert_eq!(
        first
            .items
            .iter()
            .map(|w| w.id.as_str())
            .collect::<Vec<_>>(),
        ["s4", "s3"]
    );
    let second = SubmissionStore::list(s, &mine, 2, first.next_before)
        .await
        .unwrap();
    assert_eq!(
        second
            .items
            .iter()
            .map(|w| w.id.as_str())
            .collect::<Vec<_>>(),
        ["s2", "s1"]
    );
    let queue = SubmissionFilter {
        involving: None,
        status: Some("in_review".into()),
    };
    assert_eq!(
        SubmissionStore::list(s, &queue, 10, None)
            .await
            .unwrap()
            .items
            .len(),
        2
    );

    let mut one = SubmissionStore::get(s, &"s1".into())
        .await
        .unwrap()
        .unwrap();
    one.title = "changed".into();
    SubmissionStore::replace(s, &one, 0).await.unwrap();
    assert!(matches!(
        SubmissionStore::replace(s, &one, 0).await,
        Err(CoreError::StaleRevision { .. })
    ));
    assert_eq!(
        SubmissionStore::get(s, &"s1".into())
            .await
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    let ghost = submission("nope", "u1", SubmissionStatus::Draft);
    assert!(matches!(
        SubmissionStore::replace(s, &ghost, 0).await,
        Err(CoreError::NotFound { .. })
    ));
    db.drop().await;
}

#[tokio::test]
async fn blobs_beyond_the_document_limit() {
    let Some(db) = TestDb::open().await else {
        return;
    };
    let s = &db.store;
    let big: Vec<u8> = (0..17 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
    let stored = BlobStore::put(s, &big, "application/octet-stream")
        .await
        .unwrap();
    assert_eq!(stored.bytes, big.len() as u64);
    assert_eq!(stored.sha256.len(), 64);
    assert_eq!(BlobStore::get(s, &stored.id).await.unwrap(), Some(big));
    assert_eq!(BlobStore::get(s, "missing").await.unwrap(), None);
    db.drop().await;
}

const PAPER: &str = r"\documentclass{article}
\usepackage{amsthm}
\newtheorem{theorem}{Theorem}
\newtheorem{lemma}[theorem]{Lemma}
\newtheorem{conjecture}{Conjecture}
\title{Gaps between zero runs}
\author{A. Author}
\begin{document}
\maketitle
\begin{abstract}We bound the gaps.\end{abstract}
\begin{lemma}\label{lem:count} The count is finite.\end{lemma}
\begin{proof}By compactness.\end{proof}
\begin{theorem}[Main]\label{thm:main} Every gap is at most $3$.\end{theorem}
\begin{proof}By Lemma~\ref{lem:count}.\end{proof}
\begin{conjecture} Every gap is at most $2$.\end{conjecture}
\end{document}
";

#[tokio::test]
async fn paper_flow_against_mongo() {
    let Some(db) = TestDb::open().await else {
        return;
    };
    let shared = Arc::new(db.store.clone());
    let ports = Ports {
        solving: shared.clone(),
        verifier: Arc::new(wishpool_core::ports::UnavailableVerifier),
        clock: Arc::new(SystemClock),
        people: shared.clone(),
        submissions: shared.clone(),
        endorsements: shared.clone(),
        records: shared.clone(),
        blobs: shared.clone(),
        reader: Arc::new(LatexReader),
        queue: shared.clone(),
        tasks: shared.clone(),
        contributions: shared.clone(),
        judgements: shared.clone(),
        grants: shared.clone(),
        referees: shared.clone(),
    };
    let app = App::new(
        ports,
        Policy::default(),
        BTreeSet::from([PersonId::from("admin")]),
    );
    let caller = |sub: &'static str, roles: &'static [Role]| {
        let app = app.clone();
        async move {
            let identity = |s: &str| VerifiedIdentity {
                subject: s.into(),
                name: Some(s.into()),
                email: None,
                picture: None,
            };
            app.sign_in(&identity(sub)).await.unwrap();
            if !roles.is_empty() {
                let admin = app.caller(&identity("admin")).await.unwrap();
                app.set_roles(&admin, &sub.into(), roles).await.unwrap();
            }
            app.caller(&identity(sub)).await.unwrap()
        }
    };
    let author = caller("author", &[]).await;
    let editor = caller("editor", &[Role::Editor]).await;
    let engine = caller("engine", &[Role::Reviewer]).await;
    let helper = caller("helper", &[]).await;

    let new = NewPaper {
        kind: wishpool_core::model::SubmissionKind::Paper,
        make_public_after_acceptance: true,
        typed_conjecture: None,
        ai_disclosure: AiDisclosure {
            level: AiUse::Assisted,
            statement: "A model proofread the lemma.".into(),
        },
        authors: vec![],
        msc: vec!["11B83".into()],
        doi: Some("  doi:10.48550/arXiv.2609.33421  ".into()),
        open_to_contributors: true,
    };
    let upload = Upload {
        filename: "gaps.tex".into(),
        bytes: PAPER.as_bytes().to_vec(),
    };
    let paper = app.submit_paper(&author, new, upload).await.unwrap();
    assert_eq!(paper.doi.as_deref(), Some("10.48550/arXiv.2609.33421"));
    assert_eq!(
        app.submission(&author, &paper.id).await.unwrap().doi,
        paper.doi
    );
    assert_eq!(paper.title, "Gaps between zero runs");
    assert_eq!(paper.authors[0].name, "A. Author");
    let kinds: Vec<ClaimKind> = paper.extracted.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        [ClaimKind::Lemma, ClaimKind::Theorem, ClaimKind::Conjecture]
    );
    let source = app
        .paper_file(Some(&author), &paper.id, None, false)
        .await
        .unwrap();
    assert_eq!(source.bytes, PAPER.as_bytes());

    let job = JobLease::claim(&db.store).await.unwrap().unwrap();
    assert_eq!(job.kind, JobKind::Compile);
    app.record_compilation(&engine, &paper.id, 1, Ok(b"%PDF-1.7".to_vec()))
        .await
        .unwrap();
    JobLease::complete(&db.store, &job).await.unwrap();

    let confirmations = paper
        .extracted
        .iter()
        .map(|c| ClaimConfirmation {
            depends_on_conjectures: vec![],
            id: c.id.clone(),
            kind: c.kind,
            role: c.role,
            depends_on: if c.kind == ClaimKind::Theorem {
                vec!["C1".into()]
            } else {
                vec![]
            },
            settles: None,
            excluded: false,
        })
        .collect();
    let paper = app
        .confirm_claims(&author, &paper.id, confirmations)
        .await
        .unwrap();
    assert_eq!(paper.status, SubmissionStatus::InReview);
    let next = JobLease::claim(&db.store).await.unwrap().unwrap();
    assert_eq!(next.kind, JobKind::Stage(Stage::Literature));

    // Contributors corroborate nothing alone; the editor's report and
    // judgement decide.
    let tasks = app
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
    assert_eq!(tasks.len(), 3);
    let judge_main = tasks
        .iter()
        .find(|t| t.kind == TaskKind::JudgeEscape && t.target.claim == ClaimId::from("C2"))
        .unwrap();
    app.lease_task(&helper, &judge_main.id, ContributionMode::OwnAgent)
        .await
        .unwrap();
    app.submit_contribution(
        &helper,
        &judge_main.id,
        NewContribution {
            agent: AgentInfo {
                tool: "codex".into(),
                model: "gpt-5.5".into(),
            },
            output: ContributionOutput::Judgement {
                shape: ProofShape::Content,
                witnesses: vec!["the counting bound".into()],
                rationale: "New estimate.".into(),
            },
            tokens: None,
        },
    )
    .await
    .unwrap();

    let literature = ReportDraft {
        outcome: Outcome::Pass,
        summary: "Searched OpenAlex.".into(),
        payload: StagePayload::Literature {
            prior: vec![],
            searched: vec!["OpenAlex: zero run gaps".into()],
        },
        evidence: vec![],
    };
    app.file_report(&editor, &paper.id, Stage::Literature, literature, None)
        .await
        .unwrap();
    app.judge_claim(
        &editor,
        &paper.id,
        &"C2".into(),
        ProofShape::Content,
        vec!["the counting bound".into()],
        "Agreed.".into(),
    )
    .await
    .unwrap();
    assert_eq!(app.judgements(&editor, &paper.id).await.unwrap().len(), 2);
    let contributions = app
        .list_contributions(
            ContributionFilter {
                contributor: Some("helper".into()),
                ..Default::default()
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert!(matches!(
        contributions.items[0].status,
        ContributionStatus::Verified { .. }
    ));
    app.adopt_judgements(&editor, &paper.id).await.unwrap();
    let paper = app.decide(&editor, &paper.id).await.unwrap();
    let SubmissionStatus::Accepted { record } = &paper.status else {
        panic!("expected acceptance, got {:?}", paper.decision)
    };
    assert!(record.as_str().starts_with("WP-"));

    let public = app.paper(record).await.unwrap();
    assert_eq!(
        public.summary.doi.as_deref(),
        Some("10.48550/arXiv.2609.33421")
    );
    assert_eq!(public.claims.len(), 3);
    assert!(
        serde_json::to_value(&public)
            .unwrap()
            .get("analysis")
            .is_none()
    );
    app.set_analysis_visibility(&author, &paper.id, Visibility::Public)
        .await
        .unwrap();
    let public = app.paper(&RecordId(record.0.clone())).await.unwrap();
    assert_eq!(public.claims.len(), 3);
    assert_eq!(
        app.analysis(&author, &paper.id)
            .await
            .unwrap()
            .main_with_content,
        1
    );
    assert_eq!(app.list_papers(None, None).await.unwrap().items.len(), 1);
    let pdf = app.paper_file(None, &paper.id, None, true).await.unwrap();
    assert_eq!(pdf.bytes, b"%PDF-1.7");
    assert_eq!(app.credits(None).await.unwrap()[0].verified, 1);
    db.drop().await;
}

#[tokio::test]
async fn sessions_attempts_and_job_retries() {
    let Some(db) = TestDb::open().await else {
        return;
    };
    let s = &db.store;
    let live = SessionRecord {
        subject: "u".into(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
    };
    let dead = SessionRecord {
        subject: "u".into(),
        expires_at: Utc::now() - chrono::Duration::hours(1),
    };
    s.create_session("live", &live).await.unwrap();
    s.create_session("dead", &dead).await.unwrap();
    assert_eq!(
        s.session("live").await.unwrap().unwrap().subject,
        PersonId::from("u")
    );
    assert!(
        s.session("dead").await.unwrap().is_none(),
        "expired sessions are not honoured before TTL deletion"
    );
    s.delete_session("live").await.unwrap();
    assert!(s.session("live").await.unwrap().is_none());

    let attempt = LoginAttempt {
        binding: "b".into(),
        verifier: "v".into(),
        nonce: "n".into(),
        return_to: "/".into(),
        // BSON dates hold milliseconds.
        expires_at: chrono::DateTime::from_timestamp_millis(
            Utc::now().timestamp_millis() + 600_000,
        )
        .unwrap(),
        donation: None,
    };
    s.put_attempt("st", &attempt).await.unwrap();
    assert_eq!(s.take_attempt("st").await.unwrap(), Some(attempt));
    assert_eq!(s.take_attempt("st").await.unwrap(), None);

    let sub = SubmissionId("s1".into());
    ReviewQueue::enqueue(s, &sub, JobKind::Compile)
        .await
        .unwrap();
    let job = JobLease::claim(s).await.unwrap().unwrap();
    assert_eq!((job.attempts, job.kind), (1, JobKind::Compile));
    JobLease::retry(s, &job, "boom").await.unwrap();
    assert!(
        JobLease::claim(s).await.unwrap().is_none(),
        "retry is delayed"
    );
    // A new enqueue makes the job available immediately.
    ReviewQueue::enqueue(s, &sub, JobKind::Compile)
        .await
        .unwrap();
    let again = JobLease::claim(s).await.unwrap().unwrap();
    // Completing with a stale lease does nothing.
    JobLease::complete(s, &job).await.unwrap();
    assert_eq!(
        s.raw(REVIEW_JOBS).count_documents(doc! {}).await.unwrap(),
        1
    );
    JobLease::complete(s, &again).await.unwrap();
    assert_eq!(
        s.raw(REVIEW_JOBS).count_documents(doc! {}).await.unwrap(),
        0
    );
    ReviewQueue::enqueue(s, &sub, JobKind::Stage(Stage::Literature))
        .await
        .unwrap();
    let stage = JobLease::claim(s).await.unwrap().unwrap();
    assert_eq!(stage.kind, JobKind::Stage(Stage::Literature));
    db.drop().await;
}

#[tokio::test]
async fn referee_files_are_unique_and_revision_fenced() {
    let Some(db) = TestDb::open().await else {
        return;
    };
    let s = &db.store;
    let mut file = RefereeFile::new("paper".into());
    assert!(RefereeStore::get(s, &file.id).await.unwrap().is_none());
    RefereeStore::insert(s, &file).await.unwrap();
    assert!(matches!(
        RefereeStore::insert(s, &file).await,
        Err(CoreError::Conflict(_))
    ));
    file.letters.push(FeedbackLetter {
        round: None,
        assessment: Some(Recommendation::MinorRevision),
        subject: "Feedback".into(),
        body: "Useful point".into(),
        note: String::new(),
        edited: true,
        sent_by: "editor".into(),
        sent_at: Utc::now(),
    });
    RefereeStore::replace(s, &file, 0).await.unwrap();
    let stored = RefereeStore::get(s, &file.id).await.unwrap().unwrap();
    assert_eq!(stored.revision, 1);
    assert_eq!(stored.letters, file.letters);
    assert!(matches!(
        RefereeStore::replace(s, &file, 0).await,
        Err(CoreError::StaleRevision { .. })
    ));
    db.drop().await;
}

#[tokio::test]
async fn referee_job_deferral_preserves_attempts_and_lease_fence() {
    let Some(db) = TestDb::open().await else {
        return;
    };
    let s = &db.store;
    ReviewQueue::enqueue(s, &"paper".into(), JobKind::Referee)
        .await
        .unwrap();
    let first = JobLease::claim(s).await.unwrap().unwrap();
    assert_eq!(first.attempts, 1);
    JobLease::defer(s, &first, std::time::Duration::ZERO)
        .await
        .unwrap();
    let document = s
        .raw(REVIEW_JOBS)
        .find_one(doc! { "kind": "referee" })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(document.get_i32("attempts").unwrap(), 0);
    assert!(!document.contains_key("lease"));
    assert!(!document.contains_key("lease_until"));
    assert!(document.get("last_error").unwrap().as_null().is_some());
    let second = JobLease::claim(s).await.unwrap().unwrap();
    assert_eq!(second.attempts, 1);
    assert_ne!(second.lease, first.lease);
    JobLease::defer(s, &first, std::time::Duration::ZERO)
        .await
        .unwrap();
    assert!(JobLease::claim(s).await.unwrap().is_none());
    JobLease::defer(s, &second, std::time::Duration::from_secs(60))
        .await
        .unwrap();
    assert!(JobLease::claim(s).await.unwrap().is_none());
    db.drop().await;
}

#[tokio::test]
async fn job_renewal_extends_live_lease_and_never_revives_a_stale_token() {
    let Some(db) = TestDb::open().await else {
        return;
    };
    let s = &db.store;
    ReviewQueue::enqueue(s, &"paper".into(), JobKind::Referee)
        .await
        .unwrap();
    let first = JobLease::claim(s).await.unwrap().unwrap();
    s.raw(REVIEW_JOBS).update_one(
        doc! { "lease": &first.lease },
        doc! { "$set": { "lease_until": mongodb::bson::DateTime::from_chrono(Utc::now() + chrono::Duration::minutes(2)) } },
    ).await.unwrap();
    let before = Utc::now();
    assert!(JobLease::renew(s, &first).await.unwrap());
    let renewed = s
        .raw(REVIEW_JOBS)
        .find_one(doc! { "lease": &first.lease })
        .await
        .unwrap()
        .unwrap();
    let until = renewed.get_datetime("lease_until").unwrap().to_chrono();
    assert!(until >= before + chrono::Duration::minutes(30) - chrono::Duration::milliseconds(1));
    assert_eq!(renewed.get_i32("attempts").unwrap(), 1);
    assert!(JobLease::claim(s).await.unwrap().is_none());

    s.raw(REVIEW_JOBS).update_one(
        doc! { "lease": &first.lease },
        doc! { "$set": { "lease_until": mongodb::bson::DateTime::from_chrono(Utc::now() - chrono::Duration::seconds(1)) } },
    ).await.unwrap();
    assert!(!JobLease::renew(s, &first).await.unwrap());
    JobLease::complete(s, &first).await.unwrap();
    let second = JobLease::claim(s)
        .await
        .unwrap()
        .expect("expired lease is reclaimable");
    assert_eq!(second.attempts, 2);
    assert_ne!(second.lease, first.lease);
    assert!(!JobLease::renew(s, &first).await.unwrap());
    JobLease::complete(s, &first).await.unwrap();
    JobLease::defer(s, &first, std::time::Duration::ZERO)
        .await
        .unwrap();
    JobLease::retry(s, &first, "stale worker").await.unwrap();
    assert!(JobLease::renew(s, &second).await.unwrap());
    let current = s
        .raw(REVIEW_JOBS)
        .find_one(doc! { "lease": &second.lease })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.get_i32("attempts").unwrap(), 2);
    assert_eq!(current.get_str("state").unwrap(), "leased");
    assert!(current.get("last_error").unwrap().as_null().is_some());
    JobLease::complete(s, &second).await.unwrap();
    assert!(!JobLease::renew(s, &second).await.unwrap());
    db.drop().await;
}

#[tokio::test]
async fn solving_aggregates_and_agent_names_are_unique_and_revision_fenced() {
    let Some(db) = TestDb::open().await else {
        return;
    };
    let store = &db.store;
    let agent = Entrant {
        id: "agent:one".into(),
        name: "Solver".into(),
        kind: EntrantKind::Agent,
        owner: Some(Owner {
            id: "owner".into(),
            name: "Owner".into(),
        }),
        retired: false,
        revision: 0,
    };
    let duplicate = Entrant {
        id: "agent:two".into(),
        ..agent.clone()
    };
    let (one, two) = tokio::join!(store.insert_agent(&agent), store.insert_agent(&duplicate));
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    let mut saved = store.agents().await.unwrap().pop().unwrap();
    saved.name = "Renamed".into();
    saved.revision = 1;
    store.replace_agent(&saved, 0).await.unwrap();
    assert!(matches!(
        store.replace_agent(&saved, 0).await,
        Err(CoreError::StaleRevision { .. })
    ));
    assert_eq!(store.agents().await.unwrap()[0].name, "Renamed");
    let file = SolveFile {
        id: "WP-2026-0001:C1:v1:c1".into(),
        record: "WP-2026-0001".into(),
        submission: "source".into(),
        claim: "C1".into(),
        version: 1,
        claims_revision: 1,
        revision: 0,
        admitted: true,
        failure: None,
        review_round: 1,
        task: None,
        report: None,
        audit: None,
        attempts: vec![],
        dependencies: vec![],
        downstream: 0,
    };
    SolvingStore::insert(store, &file).await.unwrap();
    assert!(SolvingStore::insert(store, &file).await.is_err());
    let mut a = file.clone();
    a.revision = 1;
    a.downstream = 1;
    let mut b = file.clone();
    b.revision = 1;
    b.downstream = 2;
    let (one, two) = tokio::join!(
        SolvingStore::replace(store, &a, 0),
        SolvingStore::replace(store, &b, 0)
    );
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    let stored = SolvingStore::get(store, &file.id).await.unwrap().unwrap();
    assert_eq!(stored.revision, 1);
    assert!([1, 2].contains(&stored.downstream));
    assert_eq!(SolvingStore::all(store).await.unwrap().len(), 1);
    db.drop().await;
}
