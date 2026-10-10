use super::*;
use crate::ports::{RecordStore, Verifier};
use async_trait::async_trait;
use chrono::{Duration, Utc};
struct FakeVerifier;
#[async_trait]
impl Verifier for FakeVerifier {
    async fn verify(&self, r: &VerificationRequest) -> CoreResult<VerificationReceipt> {
        if r.solution.contains("earlier") {
            tokio::task::yield_now().await;
        }
        Ok(VerificationReceipt {
            verdict: if r.solution.contains("reject") {
                Verdict::Rejected
            } else if r.solution.contains("disproof") {
                Verdict::Disproved
            } else {
                Verdict::Proved
            },
            reason: "independent fake Lean boundary".into(),
            target_digest: r.target_digest.clone(),
            solution_digest: format!("{:x}", sha2::Sha256::digest(r.solution.as_bytes())),
            toolchain: r.toolchain.clone(),
            axioms: vec!["propext".into()],
            checked_at: Utc::now()
                - if r.solution.contains("earlier") {
                    Duration::seconds(10)
                } else {
                    Duration::zero()
                },
            duration: 1,
        })
    }
}
use sha2::Digest;
async fn world(limit: usize) -> World {
    let stores = Arc::new(MemoryStores::default());
    let mut ports = stores.ports(Arc::new(SystemClock), Arc::new(FakeReader));
    ports.verifier = Arc::new(FakeVerifier);
    World {
        app: App::with_attempt_limit(
            ports,
            Policy::default(),
            BTreeSet::from([PersonId::from("admin")]),
            limit,
        ),
        stores,
    }
}
async fn target(w: &World, author: &Caller, reviewer: &Caller) -> (Submission, RecordId) {
    let paper = displayed_conjecture(w, author, reviewer).await;
    let s = w
        .app
        .record_lean_statement(
            reviewer,
            &paper.id,
            1,
            paper.claims_revision,
            &"C1".into(),
            "import Mathlib\ndef wishpool_target_prop : Prop := True\n".into(),
            "test".into(),
            "True".into(),
            None,
        )
        .await
        .unwrap();
    w.app
        .respond_lean_statement(
            author,
            AuthenticationMethod::CookieSession,
            &paper.id,
            &s.lean_statements[0].digest,
            true,
            String::new(),
        )
        .await
        .unwrap();
    let SubmissionStatus::Accepted { record } = &paper.status else {
        panic!()
    };
    (paper.clone(), record.clone())
}
#[tokio::test]
async fn attempts_are_private_idempotent_rate_limited_and_become_public_only_after_verification() {
    let w = world(2).await;
    let a = w.person("author", &[]).await;
    let r = w.person("reviewer", &[Role::Reviewer]).await;
    let solver = w.person("solver", &[]).await;
    let (_, record) = target(&w, &a, &r).await;
    let first = w
        .app
        .submit_attempt(
            &solver,
            &record,
            &"C1".into(),
            "proof".into(),
            None,
            "PRIVATE_NOTE".into(),
        )
        .await
        .unwrap();
    assert_eq!(first.state, "queued");
    assert!(w.app.attempt(None, &first.id).await.is_err());
    assert!(w.app.attempt(Some(&a), &first.id).await.is_err());
    assert_eq!(
        w.app
            .attempt(Some(&r), &first.id)
            .await
            .unwrap()
            .note
            .as_deref(),
        Some("PRIVATE_NOTE")
    );
    let same = w
        .app
        .submit_attempt(
            &solver,
            &record,
            &"C1".into(),
            "proof".into(),
            None,
            String::new(),
        )
        .await
        .unwrap();
    assert_eq!(first.id, same.id);
    let bad = w
        .app
        .submit_attempt(
            &solver,
            &record,
            &"C1".into(),
            "reject".into(),
            None,
            String::new(),
        )
        .await
        .unwrap();
    assert!(
        w.app
            .submit_attempt(
                &solver,
                &record,
                &"C1".into(),
                "third".into(),
                None,
                String::new()
            )
            .await
            .is_err()
    );
    assert!(w.app.verify_attempt(&solver, &first.id).await.is_err());
    w.app.verify_attempt(&r, &bad.id).await.unwrap();
    assert!(w.app.attempt(None, &bad.id).await.is_err());
    w.app.verify_attempt(&r, &first.id).await.unwrap();
    w.app.verify_attempt(&r, &first.id).await.unwrap();
    let public = serde_json::to_string(&w.app.attempt(None, &first.id).await.unwrap()).unwrap();
    assert!(!public.contains("PRIVATE_NOTE"));
    assert_eq!(
        w.app
            .my_attempts(&solver, &record, &"C1".into())
            .await
            .unwrap()
            .len(),
        2
    );
    let board = w.app.leaderboard("all", "all").await.unwrap();
    assert_eq!(board[0].score, 1);
    assert_eq!(board[0].solved, 1);
    assert_eq!(w.app.solve_notifications(&a).await.unwrap().len(), 1);
    assert!(w.app.solve_notifications(&solver).await.unwrap().is_empty());
    assert!(
        w.app
            .submit_attempt(
                &solver,
                &record,
                &"C1".into(),
                "x".repeat(1_048_577),
                None,
                String::new()
            )
            .await
            .is_err()
    );
    assert!(
        w.app
            .submit_attempt(
                &solver,
                &record,
                &"C1".into(),
                "x".into(),
                None,
                "x".repeat(2001)
            )
            .await
            .is_err()
    );
}
#[tokio::test]
async fn concurrent_verifications_award_one_point_to_the_earliest_verifier_timestamp() {
    let w = world(20).await;
    let a = w.person("author", &[]).await;
    let r = w.person("reviewer", &[Role::Reviewer]).await;
    let s1 = w.person("solver-one", &[]).await;
    let s2 = w.person("solver-two", &[]).await;
    let (_, record) = target(&w, &a, &r).await;
    let one = w
        .app
        .submit_attempt(
            &s1,
            &record,
            &"C1".into(),
            "later proof".into(),
            None,
            String::new(),
        )
        .await
        .unwrap();
    let two = w
        .app
        .submit_attempt(
            &s2,
            &record,
            &"C1".into(),
            "earlier proof".into(),
            None,
            String::new(),
        )
        .await
        .unwrap();
    let (v1, v2) = tokio::join!(
        w.app.verify_attempt(&r, &one.id),
        w.app.verify_attempt(&r, &two.id)
    );
    v1.unwrap();
    v2.unwrap();
    let board = w.app.leaderboard("all", "all").await.unwrap();
    assert_eq!(board.len(), 1);
    assert_eq!(board[0].entrant.id, s2.person.as_str());
    assert_eq!(board[0].score, 1);
    let public = w.app.public_attempts(&record, &"C1".into()).await.unwrap();
    assert_eq!(public.len(), 2);
    assert_eq!(public.iter().filter(|a| !a.also_verified).count(), 1);
    assert_eq!(public[0].id, two.id);
    assert_eq!(
        w.app
            .entrant_profile(s2.person.as_str())
            .await
            .unwrap()
            .solutions
            .len(),
        1
    );
}
#[tokio::test]
async fn agents_are_owned_unique_renameable_retireable_and_share_one_board() {
    let w = world(20).await;
    let a = w.person("author", &[]).await;
    let r = w.person("reviewer", &[Role::Reviewer]).await;
    let owner = w.person("owner", &[]).await;
    let other = w.person("other", &[]).await;
    let (_, record) = target(&w, &a, &r).await;
    let agent = w
        .app
        .create_agent(&owner, "Proof agent".into())
        .await
        .unwrap();
    assert!(
        w.app
            .create_agent(&other, "Proof agent".into())
            .await
            .is_err()
    );
    assert!(w.app.create_agent(&owner, "x".repeat(41)).await.is_err());
    assert!(
        w.app
            .submit_attempt(
                &other,
                &record,
                &"C1".into(),
                "disproof".into(),
                Some(agent.id.clone()),
                String::new()
            )
            .await
            .is_err()
    );
    assert!(
        w.app
            .update_agent(&other, &agent.id, Some("stolen".into()), false)
            .await
            .is_err()
    );
    let agent = w
        .app
        .update_agent(&owner, &agent.id, Some("Named agent".into()), false)
        .await
        .unwrap();
    let attempt = w
        .app
        .submit_attempt(
            &owner,
            &record,
            &"C1".into(),
            "disproof".into(),
            Some(agent.name.clone()),
            String::new(),
        )
        .await
        .unwrap();
    w.app.verify_attempt(&r, &attempt.id).await.unwrap();
    assert_eq!(w.app.leaderboard("all", "all").await.unwrap().len(), 1);
    assert!(w.app.leaderboard("all", "people").await.unwrap().is_empty());
    let agents = w.app.leaderboard("month", "agents").await.unwrap();
    assert_eq!(agents[0].disproved, 1);
    assert_eq!(agents[0].entrant.owner.as_ref().unwrap().id, owner.person);
    w.app
        .update_agent(&owner, &agent.id, None, true)
        .await
        .unwrap();
    assert!(
        w.app
            .submit_attempt(
                &owner,
                &record,
                &"C1".into(),
                "another".into(),
                Some(agent.id.clone()),
                String::new()
            )
            .await
            .is_err()
    );
    assert_eq!(
        w.app
            .entrant_profile(&agent.id)
            .await
            .unwrap()
            .solutions
            .len(),
        1
    );
    assert!(w.app.leaderboard("week", "all").await.is_err());
    assert!(w.app.leaderboard("all", "unknown").await.is_err());
}
#[tokio::test]
async fn visibility_and_versions_hide_solutions_and_remove_their_credit() {
    let w = world(20).await;
    let a = w.person("author", &[]).await;
    let r = w.person("reviewer", &[Role::Reviewer]).await;
    let solver = w.person("solver", &[]).await;
    let (paper, record) = target(&w, &a, &r).await;
    let attempt = w
        .app
        .submit_attempt(
            &solver,
            &record,
            &"C1".into(),
            "proof".into(),
            None,
            String::new(),
        )
        .await
        .unwrap();
    w.app.verify_attempt(&r, &attempt.id).await.unwrap();
    w.app
        .set_analysis_visibility(&a, &paper.id, Visibility::Private)
        .await
        .unwrap();
    assert!(
        w.app
            .list_conjectures(None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(w.app.attempt(None, &attempt.id).await.is_err());
    assert!(w.app.leaderboard("all", "all").await.unwrap().is_empty());
    w.app
        .set_analysis_visibility(&a, &paper.id, Visibility::Public)
        .await
        .unwrap();
    assert_eq!(w.app.leaderboard("all", "all").await.unwrap().len(), 1);
    w.app
        .upload_version(
            &a,
            &paper.id,
            Upload {
                filename: "c.tex".into(),
                bytes: b"new".to_vec(),
            },
            String::new(),
        )
        .await
        .unwrap();
    assert!(w.app.target(&record, &"C1".into()).await.is_err());
    assert!(w.app.attempt(None, &attempt.id).await.is_err());
    assert!(w.app.leaderboard("all", "all").await.unwrap().is_empty());
}
#[tokio::test]
async fn paper_problem_policy_excludes_private_settled_and_failed_audit_candidates() {
    let w = world(20).await;
    let a = w.person("author", &[]).await;
    let r = w.person("reviewer", &[Role::Reviewer]).await;
    let original = displayed_conjecture(&w, &a, &r).await;
    let original_record = w
        .stores
        .for_submission(&original.id)
        .await
        .unwrap()
        .unwrap();
    let mut paper = original.clone();
    paper.id = "paper-problems".into();
    paper.kind = SubmissionKind::Paper;
    let record: RecordId = "WP-2026-0500".into();
    paper.status = SubmissionStatus::Accepted {
        record: record.clone(),
    };
    let mut settled = paper.claims[0].clone();
    settled.id = "C2".into();
    settled.has_proof = true;
    paper.claims.push(settled);
    w.app.ports.submissions.insert(&paper).await.unwrap();
    let mut rec = original_record;
    rec.id = record.clone();
    rec.submission = paper.id.clone();
    rec.kind = SubmissionKind::Paper;
    rec.publication = Some(Box::new(paper.clone()));
    RecordStore::insert(&*w.stores, &rec).await.unwrap();
    let files = w.app.open_problem_files(&r, &paper.id).await.unwrap();
    assert_eq!(files.len(), 1);
    assert!(!files[0].admitted);
    assert!(
        !w.app
            .list_conjectures(None, None)
            .await
            .unwrap()
            .items
            .iter()
            .any(|c| c.record == record)
    );
    let audit = |reading| RefereeAudit {
        verdict: Recommendation::Accept,
        agrees_with_referee: true,
        summary: "PRIVATE_AUDIT".into(),
        claims: vec![AuditedClaim {
            claim: "C1".into(),
            conjecture: Some(reading),
            correctness: Correctness::NotChecked,
            comment: "PRIVATE_COMMENT".into(),
            shape: None,
            witnesses: vec![],
            known: None,
            referee_agreed: true,
        }],
        concerns: vec![],
    };
    let mut failed = files[0].clone();
    let mut reading = open_reading();
    reading.well_posed = false;
    failed.audit = Some(audit(reading));
    w.app.record_problem_review(&r, failed).await.unwrap();
    assert!(!w.app.open_problem_files(&r, &paper.id).await.unwrap()[0].admitted);
    // A separate revision fences a stale old audit, and rechecks fresh inputs.
    let mut current = w.app.load(&paper.id).await.unwrap();
    current.claims_revision += 1;
    w.app.save(&mut current).await.unwrap();
    assert!(
        w.app
            .record_problem_review(&r, files[0].clone())
            .await
            .is_err()
    );
    let mut passed = w
        .app
        .open_problem_files(&r, &paper.id)
        .await
        .unwrap()
        .remove(0);
    passed.audit = Some(audit(open_reading()));
    w.app.record_problem_review(&r, passed).await.unwrap();
    let listed = w.app.list_conjectures(None, None).await.unwrap();
    let c = listed.items.iter().find(|c| c.record == record).unwrap();
    assert_eq!(c.source, "from WP-2026-0500");
    assert_eq!(c.claim.as_str(), "C1");
    let payload = serde_json::to_string(&listed).unwrap();
    assert!(!payload.contains("PRIVATE_"));
    w.app
        .set_analysis_visibility(&a, &paper.id, Visibility::Private)
        .await
        .unwrap();
    assert!(
        w.app
            .open_problem_files(&r, &paper.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        !w.app
            .list_conjectures(None, None)
            .await
            .unwrap()
            .items
            .iter()
            .any(|c| c.record == record)
    );
}
#[tokio::test]
async fn unconfirmed_targets_reject_attempts_and_legacy_conversion_requires_new_confirmation() {
    let w = world(20).await;
    let a = w.person("author", &[]).await;
    let r = w.person("reviewer", &[Role::Reviewer]).await;
    let paper = displayed_conjecture(&w, &a, &r).await;
    let SubmissionStatus::Accepted { record } = &paper.status else {
        panic!()
    };
    assert!(
        w.app
            .submit_attempt(
                &a,
                record,
                &"C1".into(),
                "proof".into(),
                None,
                String::new()
            )
            .await
            .is_err()
    );
    let mut current = w.app.load(&paper.id).await.unwrap();
    let legacy = "import Mathlib\ntheorem wishpool_target : True := by sorry";
    let old = format!("{:x}", sha2::Sha256::digest(legacy.as_bytes()));
    current.lean_statements.push(LeanStatementAttempt {
        claim: "C1".into(),
        version: 1,
        claims_revision: current.claims_revision,
        lean: legacy.into(),
        digest: old.clone(),
        toolchain: "old".into(),
        reading: "True".into(),
        response: LeanStatementResponse::Confirmed {
            author: a.person.clone(),
            at: Utc::now(),
        },
        created_at: Utc::now(),
    });
    w.app.save(&mut current).await.unwrap();
    w.app
        .replace_legacy_target(
            &r,
            &paper.id,
            &old,
            Some((
                "import Mathlib\ndef wishpool_target_prop : Prop := True\n".into(),
                "test".into(),
            )),
        )
        .await
        .unwrap();
    assert!(w.app.target(record, &"C1".into()).await.is_err());
    let current = w.app.load(&paper.id).await.unwrap();
    assert_ne!(current.lean_statements[0].digest, old);
    assert_eq!(
        current.lean_statements[0].response,
        LeanStatementResponse::AwaitingAuthor
    );
    assert!(
        w.app
            .respond_lean_statement(
                &a,
                AuthenticationMethod::CookieSession,
                &paper.id,
                &old,
                true,
                String::new()
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn superseded_pending_attempts_are_terminal_private_and_not_requeued() {
    let w = world(20).await;
    let a = w.person("author", &[]).await;
    let r = w.person("reviewer", &[Role::Reviewer]).await;
    let solver = w.person("solver", &[]).await;
    let (paper, record) = target(&w, &a, &r).await;
    let attempt = w
        .app
        .submit_attempt(
            &solver,
            &record,
            &"C1".into(),
            "proof".into(),
            None,
            String::new(),
        )
        .await
        .unwrap();
    while w.stores.take_job().await.is_some() {}
    w.app
        .upload_version(
            &a,
            &paper.id,
            Upload {
                filename: "c.tex".into(),
                bytes: b"new version".to_vec(),
            },
            String::new(),
        )
        .await
        .unwrap();
    for _ in 0..2 {
        w.app.reconcile_attempts().await.unwrap();
    }
    assert!(
        !w.stores
            .queued()
            .await
            .iter()
            .any(|(_, kind)| *kind == JobKind::VerifyAttempt)
    );
    let private = w.app.attempt(Some(&solver), &attempt.id).await.unwrap();
    assert_eq!(private.state, "superseded");
    assert!(private.reason.unwrap().contains("current target"));
    assert!(private.receipt.is_none());
    assert!(w.app.attempt(None, &attempt.id).await.is_err());
    w.app.verify_attempt(&r, &attempt.id).await.unwrap();
    assert!(w.app.leaderboard("all", "all").await.unwrap().is_empty());
}

#[tokio::test]
async fn leaderboard_ranks_by_score_then_earlier_last_solve_and_uses_calendar_month() {
    use chrono::Datelike;
    let w = world(20).await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("reviewer", &[Role::Reviewer]).await;
    let ada = w.person("ada", &[]).await;
    let bob = w.person("bob", &[]).await;
    let owner = w.person("owner", &[]).await;
    let agent = w
        .app
        .create_agent(&owner, "Agent before rename".into())
        .await
        .unwrap();
    let start = Utc::now()
        .date_naive()
        .with_day(1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    for (solver, agent_id, at) in [
        (&ada, None, start - Duration::seconds(1)),
        (&ada, None, start),
        (&bob, None, start + Duration::seconds(1)),
        (&bob, None, start + Duration::seconds(2)),
        (&owner, Some(agent.id.clone()), start + Duration::seconds(3)),
    ] {
        let (_, record) = target(&w, &author, &reviewer).await;
        let a = w
            .app
            .submit_attempt(
                solver,
                &record,
                &"C1".into(),
                "proof".into(),
                agent_id,
                String::new(),
            )
            .await
            .unwrap();
        w.app.verify_attempt(&reviewer, &a.id).await.unwrap();
        let mut file = w.app.solve_file(&record, &"C1".into()).await.unwrap();
        file.attempts[0].receipt.as_mut().unwrap().checked_at = at;
        w.app.save_solve_file(&mut file).await.unwrap();
    }
    w.app
        .update_agent(&owner, &agent.id, Some("Renamed agent".into()), true)
        .await
        .unwrap();
    let all = w.app.leaderboard("all", "all").await.unwrap();
    assert_eq!(
        all.iter()
            .map(|r| (r.rank, r.entrant.id.as_str(), r.score))
            .collect::<Vec<_>>(),
        [
            (1, ada.person.as_str(), 2),
            (2, bob.person.as_str(), 2),
            (3, agent.id.as_str(), 1)
        ]
    );
    assert_eq!(all[2].entrant.name, "Renamed agent");
    assert!(all[2].entrant.retired);
    let month = w.app.leaderboard("month", "people").await.unwrap();
    assert_eq!(
        month
            .iter()
            .map(|r| (r.rank, r.entrant.id.as_str(), r.score))
            .collect::<Vec<_>>(),
        [(1, bob.person.as_str(), 2), (2, ada.person.as_str(), 1)]
    );
    let agents = w.app.leaderboard("month", "agents").await.unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].rank, 1);
    assert_eq!(
        w.app.entrant_profile(&agent.id).await.unwrap().entrant.name,
        "Renamed agent"
    );
}

#[tokio::test]
async fn confirmed_external_edges_are_revision_bound_and_count_distinct_public_works() {
    let w = world(20).await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("reviewer", &[Role::Reviewer]).await;
    let (_, record) = target(&w, &author, &reviewer).await;
    let target_file = w.app.solve_file(&record, &"C1".into()).await.unwrap();
    let source = w.submit(&author, "tt", false).await;
    let mut confirmations: Vec<_> = source
        .extracted
        .iter()
        .map(|c| ClaimConfirmation {
            id: c.id.clone(),
            kind: c.kind,
            role: c.role,
            depends_on: vec![],
            depends_on_conjectures: vec![],
            settles: None,
            excluded: false,
        })
        .collect();
    let external = ConjectureRef {
        record: record.clone(),
        claim: "C1".into(),
    };
    confirmations[0].depends_on_conjectures = vec![external.clone(), external.clone()];
    confirmations[1].depends_on_conjectures = vec![external];
    let mut source = w
        .app
        .confirm_claims(&author, &source.id, confirmations)
        .await
        .unwrap();
    assert_eq!(source.conjecture_dependencies.len(), 2);
    assert_eq!(
        source.conjecture_dependencies[0].target_version,
        target_file.version
    );
    assert_eq!(
        source.conjecture_dependencies[0].target_claims_revision,
        target_file.claims_revision
    );
    let source_record: RecordId = "WP-2026-0900".into();
    source.status = SubmissionStatus::Accepted {
        record: source_record.clone(),
    };
    source.analysis_visibility = Visibility::Public;
    w.app.save(&mut source).await.unwrap();
    let base = w
        .stores
        .for_submission(&target_file.submission)
        .await
        .unwrap()
        .unwrap();
    let mut rec = base.clone();
    rec.id = source_record;
    rec.submission = source.id.clone();
    rec.publication = Some(Box::new(source.clone()));
    RecordStore::insert(&*w.stores, &rec).await.unwrap();
    w.app.refresh_dependency_counts().await.unwrap();
    let file = w.app.solve_file(&record, &"C1".into()).await.unwrap();
    assert_eq!(file.downstream, 1);
    assert_eq!(file.dependencies.len(), 2);
    assert_eq!(file.dependencies[0].version, 1);
    assert_eq!(file.dependencies[0].claims_revision, source.claims_revision);
    w.app
        .set_analysis_visibility(&author, &source.id, Visibility::Private)
        .await
        .unwrap();
    w.app.refresh_dependency_counts().await.unwrap();
    assert_eq!(
        w.app
            .solve_file(&record, &"C1".into())
            .await
            .unwrap()
            .downstream,
        0
    );
}

#[tokio::test]
async fn reconciliation_repairs_requested_problem_jobs_without_backfilling_legacy_papers() {
    let w = world(20).await;
    let author = w.person("author", &[]).await;
    let reviewer = w.person("reviewer", &[Role::Reviewer]).await;
    let admin = w.person("admin", &[]).await;
    let (original, _) = target(&w, &author, &reviewer).await;
    let mut paper = original.clone();
    paper.id = "legacy-paper".into();
    paper.kind = SubmissionKind::Paper;
    paper.lean_statements.clear();
    paper.problem_check_requested = false;
    let record: RecordId = "WP-2026-0910".into();
    paper.status = SubmissionStatus::Accepted {
        record: record.clone(),
    };
    w.app.ports.submissions.insert(&paper).await.unwrap();
    let mut rec = w
        .stores
        .for_submission(&original.id)
        .await
        .unwrap()
        .unwrap();
    rec.id = record;
    rec.submission = paper.id.clone();
    rec.publication = Some(Box::new(paper.clone()));
    RecordStore::insert(&*w.stores, &rec).await.unwrap();
    while w.stores.take_job().await.is_some() {}
    w.app.reconcile_attempts().await.unwrap();
    assert!(
        !w.stores
            .queued()
            .await
            .iter()
            .any(|(id, kind)| id == &paper.id && *kind == JobKind::OpenProblems)
    );
    w.app.queue_open_problems(&admin, &paper.id).await.unwrap();
    assert!(w.app.load(&paper.id).await.unwrap().problem_check_requested);
    // Simulate loss of the queued job after the durable request was saved.
    while w.stores.take_job().await.is_some() {}
    w.app.reconcile_attempts().await.unwrap();
    assert!(
        w.stores
            .queued()
            .await
            .iter()
            .any(|(id, kind)| id == &paper.id && *kind == JobKind::OpenProblems)
    );
}
