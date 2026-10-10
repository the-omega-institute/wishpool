use std::{collections::BTreeSet, sync::Arc};

use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{StatusCode, header},
    middleware::{self, Next},
    response::Response,
};
use tower::ServiceExt;
use wishpool_core::{
    app::App,
    memory::MemoryStores,
    model::{Caller, ClaimKind, VerifiedIdentity},
    policy::Policy,
    ports::{PaperReader, ReadPaper, ReadStatement, SystemClock},
};

use crate::AuthenticatedCaller;

/// Test-only authentication: `x-test-subject` names the caller.
async fn test_auth(
    axum::extract::State(app): axum::extract::State<Arc<App>>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(subject) = request
        .headers()
        .get("x-test-subject")
        .and_then(|v| v.to_str().ok())
    {
        let identity = VerifiedIdentity {
            subject: subject.into(),
            name: Some(subject.into()),
            email: None,
            picture: None,
        };
        let caller: Caller = app.caller(&identity).await.unwrap();
        request.extensions_mut().insert(AuthenticatedCaller(caller));
    }
    next.run(request).await
}

/// Reads any upload as a paper with one theorem and one lemma.
struct OneTheorem;

impl PaperReader for OneTheorem {
    fn read(&self, bytes: &[u8], _filename: &str) -> Result<ReadPaper, String> {
        if bytes.starts_with(b"garbage") {
            return Err("no main .tex file".into());
        }
        let statement = |kind, name: &str| ReadStatement {
            kind,
            display_name: name.into(),
            title: None,
            latex_label: None,
            body: "$x$".into(),
            has_proof: true,
            section: None,
        };
        Ok(ReadPaper {
            main_file: "main.tex".into(),
            title: Some("On gaps".into()),
            authors: vec!["A. Author".into()],
            abstract_text: Some("We bound gaps.".into()),
            statements: vec![
                statement(ClaimKind::Theorem, "Theorem"),
                statement(ClaimKind::Lemma, "Lemma"),
            ],
            macros: Default::default(),
            warnings: vec![],
        })
    }
}

fn router() -> Router {
    let stores = Arc::new(MemoryStores::default());
    let app = App::new(
        stores.ports(Arc::new(SystemClock), Arc::new(OneTheorem)),
        Policy::default(),
        BTreeSet::new(),
    );
    crate::router(app.clone()).layer(middleware::from_fn_with_state(app, test_auth))
}

async fn send(router: &Router, request: Request) -> (StatusCode, serde_json::Value, String) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json, content_type)
}

async fn call(
    router: &Router,
    method: &str,
    uri: &str,
    subject: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value, String) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(subject) = subject {
        builder = builder.header("x-test-subject", subject);
    }
    let request = match body {
        Some(json) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string())),
        None => builder.body(Body::empty()),
    }
    .unwrap();
    send(router, request).await
}

const BOUNDARY: &str = "wishpool-test-boundary";

fn multipart(parts: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, filename, content) in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match filename {
            Some(f) => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{f}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            ),
        }
        body.extend_from_slice(content);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

async fn upload(
    router: &Router,
    subject: &str,
    source: &[u8],
) -> (StatusCode, serde_json::Value, String) {
    let metadata = serde_json::json!({
        "ai_disclosure": { "level": "assisted", "statement": "A model proofread section 2." },
        "open_to_contributors": true
    })
    .to_string();
    let body = multipart(&[
        ("metadata", None, metadata.as_bytes()),
        ("source", Some("paper.tex"), source),
    ]);
    let request = Request::builder()
        .method("POST")
        .uri("/submissions")
        .header("x-test-subject", subject)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap();
    send(router, request).await
}

#[tokio::test]
async fn policy_is_public() {
    let router = router();
    let (status, json, _) = call(&router, "GET", "/policy", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["policy"]["max_active_per_author"], 3);
    assert_eq!(json["stages"].as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn anonymous_me_is_a_problem_document() {
    let router = router();
    let (status, json, content_type) = call(&router, "GET", "/me", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(content_type, "application/problem+json");
    assert_eq!(json["code"], "not_authenticated");
}

#[tokio::test]
async fn upload_and_confirm_round_trip() {
    let router = router();
    let (status, json, _) = upload(&router, "author", b"\\documentclass{article}").await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    assert_eq!(json["status"]["state"], "draft");
    assert_eq!(json["extracted"].as_array().unwrap().len(), 2);
    let id = json["id"].as_str().unwrap().to_owned();

    // Private to the author until accepted.
    let (status, _, _) = call(
        &router,
        "GET",
        &format!("/submissions/{id}"),
        Some("stranger"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = call(
        &router,
        "GET",
        &format!("/submissions/{id}/files/source"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = call(
        &router,
        "GET",
        &format!("/submissions/{id}/files/source"),
        Some("author"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let claims = serde_json::json!({ "claims": [
        { "id": "C1", "kind": "theorem", "role": "main" },
        { "id": "C2", "kind": "lemma", "role": "supporting", "depends_on": [] }
    ]});
    let (status, json, _) = call(
        &router,
        "POST",
        &format!("/submissions/{id}/claims"),
        Some("stranger"),
        Some(claims.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{json}");
    let (status, json, _) = call(
        &router,
        "POST",
        &format!("/submissions/{id}/claims"),
        Some("author"),
        Some(claims),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["status"]["state"], "in_review");
    assert_eq!(json["claims"].as_array().unwrap().len(), 2);

    // The author opted in: signed-in contributors see the tasks.
    let (status, _, _) = call(&router, "GET", "/tasks", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, json, _) = call(
        &router,
        "GET",
        &format!("/tasks?submission={id}"),
        Some("helper"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["items"].as_array().unwrap().len(), 3);
    let task = json["items"][0]["id"].as_str().unwrap().to_owned();
    let (status, json, _) = call(
        &router,
        "POST",
        &format!("/tasks/{task}/lease"),
        Some("helper"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let (status, _, _) = call(
        &router,
        "POST",
        &format!("/tasks/{task}/lease"),
        Some("author"),
        Some(serde_json::json!({"mode": "hosted"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, json, _) = call(
        &router,
        "GET",
        &format!("/submissions/{id}/analysis"),
        Some("author"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["main_results"], 1);
    let (status, json, _) = call(&router, "GET", "/papers", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["items"], serde_json::json!([]));
}

#[tokio::test]
async fn malformed_input_is_unprocessable() {
    let router = router();
    let (status, json, _) = upload(&router, "author", b"garbage").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["code"], "invalid");
    assert!(json["detail"].as_str().unwrap().contains("no main .tex"));

    let (status, _, _) = call(
        &router,
        "POST",
        "/submissions/x/stages/nonsense/reports",
        Some("poser"),
        Some(serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _, _) = call(&router, "GET", "/submissions/x/files/zip", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn submission_queue_requires_a_review_role() {
    let router = router();
    let (status, _, _) = call(
        &router,
        "GET",
        "/submissions?scope=queue",
        Some("someone"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, json, _) = call(&router, "GET", "/submissions", Some("someone"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["items"], serde_json::json!([]));
}

#[tokio::test]
async fn referee_routes_project_visibility_roles_and_letter_validation() {
    use wishpool_core::{
        ids::SubmissionId,
        model::{Role, RoundUpdate, Step, StepState},
    };
    let stores = Arc::new(MemoryStores::default());
    let app = App::new(
        stores.ports(Arc::new(SystemClock), Arc::new(OneTheorem)),
        Policy::default(),
        BTreeSet::new(),
    );
    let editor = app
        .ensure_service_account(&"editor".into(), "Editor", Role::Editor)
        .await
        .unwrap();
    let reviewer = app
        .ensure_service_account(&"referee".into(), "Referee", Role::Reviewer)
        .await
        .unwrap();
    let router =
        crate::router(app.clone()).layer(middleware::from_fn_with_state(app.clone(), test_auth));
    let (status, paper, _) = upload(&router, "author", b"paper").await;
    assert_eq!(status, StatusCode::CREATED);
    let id = SubmissionId::from(paper["id"].as_str().unwrap());
    let base = format!("/submissions/{id}/referee");
    assert_eq!(
        call(&router, "GET", &base, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&router, "GET", &base, Some("stranger"), None).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, view, _) = call(&router, "GET", &base, Some("author"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["rounds"], serde_json::json!([]));
    assert_eq!(
        call(
            &router,
            "POST",
            &format!("{base}/restart"),
            Some("author"),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let letters = format!("{base}/letters");
    assert_eq!(
        call(
            &router,
            "POST",
            &letters,
            Some("author"),
            Some(serde_json::json!({"subject":"s","body":"b"}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &router,
            "POST",
            &letters,
            Some("editor"),
            Some(serde_json::json!({"subject":"s","body":" "}))
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, letter, _) = call(
        &router,
        "POST",
        &letters,
        Some("editor"),
        Some(serde_json::json!({"subject":"Feedback","body":"A useful observation."})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(letter["note"], "");
    assert_eq!(letter["edited"], true);
    app.record_compilation(&reviewer, &id, 1, Ok(b"%PDF".to_vec()))
        .await
        .unwrap();
    let caller = app
        .caller(&VerifiedIdentity {
            subject: "author".into(),
            name: None,
            email: None,
            picture: None,
        })
        .await
        .unwrap();
    let submission = app.submission(&caller, &id).await.unwrap();
    let confirmations = submission
        .extracted
        .iter()
        .map(|c| wishpool_core::model::ClaimConfirmation {
            id: c.id.clone(),
            kind: c.kind,
            role: c.role,
            depends_on: vec![],
            settles: None,
            excluded: false,
        })
        .collect();
    app.confirm_claims(&caller, &id, confirmations)
        .await
        .unwrap();
    app.begin_referee_round(&reviewer, &id).await.unwrap();
    assert_eq!(
        call(
            &router,
            "POST",
            &format!("{base}/restart"),
            Some("editor"),
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let mut referee = Step::pending();
    referee.state = StepState::Failed {
        reason: "failed".into(),
        detail: None,
        retryable: false,
        at: submission.created_at,
    };
    app.update_referee_round(&reviewer, &id, 1, RoundUpdate::Referee(referee))
        .await
        .unwrap();
    let mut audit = Step::pending();
    audit.state = StepState::Skipped {
        reason: "failed referee".into(),
    };
    app.update_referee_round(&reviewer, &id, 1, RoundUpdate::Audit(audit))
        .await
        .unwrap();
    let mut advice = Step::pending();
    advice.state = StepState::Skipped {
        reason: "failed referee".into(),
    };
    app.update_referee_round(&reviewer, &id, 1, RoundUpdate::Advice(advice))
        .await
        .unwrap();
    let mut letter = Step::pending();
    letter.state = StepState::Skipped {
        reason: "failed referee".into(),
    };
    app.update_referee_round(&reviewer, &id, 1, RoundUpdate::Letter(letter))
        .await
        .unwrap();
    let mut formal = Step::pending();
    formal.state = StepState::Skipped {
        reason: "failed referee".into(),
    };
    app.update_referee_round(&reviewer, &id, 1, RoundUpdate::Formal(formal))
        .await
        .unwrap();
    let (status, file, _) = call(
        &router,
        "POST",
        &format!("{base}/restart"),
        Some("editor"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(file["rounds"].as_array().unwrap().len(), 2);
    let (_, view, _) = call(&router, "GET", &base, Some("author"), None).await;
    assert_eq!(view["rounds"].as_array().unwrap().len(), 2);
    assert!(view["rounds"][0].get("advice").is_none());
    assert!(view["rounds"][0].get("letter").is_none());
    assert_eq!(view["letters"].as_array().unwrap().len(), 1);
    assert_eq!(app.referee(&editor, &id).await.unwrap().rounds.len(), 2);
}
