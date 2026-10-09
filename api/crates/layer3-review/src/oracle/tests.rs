use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde_json::{Value, json};

use super::*;

#[test]
fn parses_server_and_cli_envelopes() {
    let submitted = parse_task(r#"{"task_id":"t1","queue_position":2,"deduplicated":true}"#)
        .unwrap()
        .submitted()
        .unwrap();
    assert_eq!(submitted.task, "t1");
    assert_eq!(submitted.queue_position, Some(2));
    for (text, expected) in [
        (
            r#"{"status":"queued","queue_position":4}"#,
            OracleStatus::Queued { position: Some(4) },
        ),
        (r#"{"status":"dispatched"}"#, OracleStatus::Running),
        (
            r#"{"status":"completed","response":"a full answer"}"#,
            OracleStatus::Completed {
                text: "a full answer".into(),
            },
        ),
        (
            r#"{"status":"failed","failure_reason":"prompt_delivery_uncertain","failure_detail":"page_crashed@waiting_response"}"#,
            OracleStatus::Failed {
                reason: "prompt_delivery_uncertain".into(),
                detail: Some("page_crashed@waiting_response".into()),
            },
        ),
        (r#"{"status":"cancelled"}"#, OracleStatus::Cancelled),
    ] {
        assert_eq!(parse_task(text).unwrap().status().unwrap(), expected);
    }
    assert!(parse_task("{}").unwrap().submitted().is_err());
    assert!(
        parse_task(r#"{"status":"completed"}"#)
            .unwrap()
            .status()
            .is_err()
    );
    assert!(
        parse_task(r#"{"status":"unknown"}"#)
            .unwrap()
            .status()
            .is_err()
    );
    assert!(parse_task("not JSON").is_err());
}

async fn serve(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{address}")
}

fn request() -> OracleRequest {
    OracleRequest {
        prompt: "Read the paper".into(),
        pdf: Some(("p.pdf".into(), b"%PDF".to_vec())),
        client_ref: "wishpool:p:r1:v1:c1".into(),
        tag: "wishpool".into(),
    }
}

#[tokio::test]
async fn http_sends_payload_and_polls() {
    let token = uuid_token();
    let expected_token = format!("Bearer {token}");
    let router = Router::new()
        .route(
            "/api/v1/oracle/pools/test/tasks",
            post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let expected_token = expected_token.clone();
                async move {
                    assert_eq!(headers["authorization"], expected_token);
                    assert_eq!(body["prompt"], "Read the paper");
                    assert!(body.get("model").is_none());
                    assert_eq!(body["client_ref"], "wishpool:p:r1:v1:c1:test");
                    assert_eq!(body["tag"], "wishpool");
                    assert_eq!(body["pdf_name"], "p.pdf");
                    assert_eq!(body["pdf_base64"], STANDARD.encode(b"%PDF"));
                    Json(json!({"task_id":"t1","queue_position":3}))
                }
            }),
        )
        .route(
            "/api/v1/oracle/tasks/t1",
            get(|| async { Json(json!({"status":"completed","response":"report"})) }),
        );
    let oracle = OracleHttp {
        base_url: serve(router).await,
        token,
        pools: vec!["test".into()],
    };
    assert_eq!(oracle.submit(&request()).await.unwrap().task, "t1");
    assert_eq!(
        oracle.poll("t1").await.unwrap(),
        OracleStatus::Completed {
            text: "report".into()
        }
    );
}

fn uuid_token() -> String {
    // Runtime-generated test material, never a fixture credential.
    tempfile::tempdir()
        .unwrap()
        .path()
        .to_string_lossy()
        .to_string()
}

#[tokio::test]
async fn http_limits_and_transient_capacity_error() {
    let oracle = OracleHttp {
        base_url: serve(Router::new().route(
            "/api/v1/oracle/pools/test/tasks",
            post(|| async { (StatusCode::TOO_MANY_REQUESTS, "full") }),
        ))
        .await,
        token: uuid_token(),
        pools: vec!["test".into()],
    };
    assert!(matches!(
        oracle.submit(&request()).await,
        Err(ReviewError::Provider { status: 429, .. })
    ));
    let mut input = request();
    input.prompt = "x".repeat(500_001);
    assert!(matches!(
        oracle.submit(&input).await,
        Err(ReviewError::Output(_))
    ));
    input.prompt = "small".into();
    input.pdf = Some(("p.pdf".into(), vec![0; 9_000_001]));
    assert!(matches!(
        oracle.submit(&input).await,
        Err(ReviewError::Output(_))
    ));
    input.pdf = Some(("p.pdf".into(), vec![0; 9_000_000]));
    assert!(validate(&input).is_ok());
}

#[cfg(unix)]
#[tokio::test]
async fn cli_uses_per_request_files_and_server_json() {
    use std::os::unix::fs::PermissionsExt;
    let work = tempfile::tempdir().unwrap();
    let program = work.path().join("fake-nyxid");
    std::fs::write(&program, r#"#!/bin/sh
[ "$1" = "oracle" ] || exit 2
if [ "$2" = "ask" ]; then
  [ "$3" = "test-pool" ] || exit 3
  [ "$4" = "--file" ] || exit 4
  [ -f "$5" ] || exit 5
  [ "$6" = "--pdf" ] || exit 6
  [ -f "$7" ] || exit 7
  [ "$8" = "--client-ref" ] || exit 8
  [ "$9" = "wishpool:p:r1:v1:c1:test-pool" ] || exit 9
  case "$*" in *--model*) exit 14 ;; esac
  printf '%s' '{"task_id":"task-1","queue_position":2}'
else
  [ "$2" = "result" ] || exit 10
  [ "$3" = "task-1" ] || exit 11
  [ "$4" = "--output" ] || exit 12
  [ "$5" = "json" ] || exit 13
  printf '%s' '{"status":"failed","failure_reason":"model_unavailable","failure_detail":"page_crashed"}'
fi
"#).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let oracle = OracleCli {
        program,
        pools: vec!["test-pool".into()],
        work_dir: work.path().join("requests"),
    };
    let submitted = oracle.submit(&request()).await.unwrap();
    assert_eq!(submitted.task, "task-1");
    assert_eq!(
        oracle.poll(&submitted.task).await.unwrap(),
        OracleStatus::Failed {
            reason: "model_unavailable".into(),
            detail: Some("page_crashed".into())
        }
    );
    assert!(
        std::fs::read_dir(&oracle.work_dir)
            .unwrap()
            .next()
            .is_none()
    );
}

#[tokio::test]
async fn validates_character_and_base64_limits_before_transport() {
    let oracle = OracleHttp {
        base_url: ":invalid-url".into(),
        token: uuid_token(),
        pools: vec!["test".into()],
    };
    let mut input = request();
    input.pdf = None;
    input.prompt = "é".repeat(500_000);
    // A million UTF-8 bytes are still only 500,000 characters. Passing
    // validation reaches URL parsing, without any network request.
    assert!(
        matches!(oracle.submit(&input).await, Err(ReviewError::Output(message)) if !message.contains("characters"))
    );
    input.prompt.push('é');
    assert!(
        matches!(oracle.submit(&input).await, Err(ReviewError::Output(message)) if message.contains("500,000 characters"))
    );
    input.prompt = "small".into();
    input.pdf = Some(("p.pdf".into(), vec![0; 9_000_001]));
    assert!(
        matches!(oracle.submit(&input).await, Err(ReviewError::Output(message)) if message.contains("12,000,000 bytes"))
    );
    input.pdf = Some(("p.pdf".into(), vec![0; 9_000_000]));
    assert!(
        matches!(oracle.submit(&input).await, Err(ReviewError::Output(message)) if !message.contains("12,000,000 bytes"))
    );
}

#[tokio::test]
async fn pools_race_and_first_answer_wins() {
    let router = Router::new()
        .route(
            "/api/v1/oracle/pools/{pool}/tasks",
            post(
                |axum::extract::Path(pool): axum::extract::Path<String>,
                 Json(body): Json<Value>| async move {
                    assert_eq!(body["client_ref"], format!("wishpool:p:r1:v1:c1:{pool}"));
                    match pool.as_str() {
                        "full" => (StatusCode::TOO_MANY_REQUESTS, Json(json!({}))),
                        "slow" => (
                            StatusCode::OK,
                            Json(json!({"task_id":"ta","queue_position":30})),
                        ),
                        _ => (
                            StatusCode::OK,
                            Json(json!({"task_id":"tb","queue_position":4})),
                        ),
                    }
                },
            ),
        )
        .route(
            "/api/v1/oracle/tasks/ta",
            get(|| async { Json(json!({"status":"queued","queue_position":29})) }),
        )
        .route(
            "/api/v1/oracle/tasks/tb",
            get(|| async { Json(json!({"status":"completed","response":"report"})) }),
        )
        .route(
            "/api/v1/oracle/tasks/tc",
            get(|| async { Json(json!({"status":"queued","queue_position":7})) }),
        )
        .route(
            "/api/v1/oracle/tasks/tf",
            get(|| async { Json(json!({"status":"failed","failure_reason":"model_unavailable"})) }),
        );
    let oracle = OracleHttp {
        base_url: serve(router).await,
        token: uuid_token(),
        pools: vec!["full".into(), "slow".into(), "fast".into()],
    };
    let submitted = oracle.submit(&request()).await.unwrap();
    assert_eq!(submitted.task, "ta,tb");
    assert_eq!(submitted.queue_position, Some(4));
    assert_eq!(
        oracle.poll("ta,tb").await.unwrap(),
        OracleStatus::Completed {
            text: "report".into()
        }
    );
    assert_eq!(
        oracle.poll("ta,tc").await.unwrap(),
        OracleStatus::Queued { position: Some(7) }
    );
    assert_eq!(
        oracle.poll("tf,tc").await.unwrap(),
        OracleStatus::Queued { position: Some(7) }
    );
    assert!(matches!(
        oracle.poll("tf").await.unwrap(),
        OracleStatus::Failed { .. }
    ));
    let only_full = OracleHttp {
        pools: vec!["full".into()],
        ..oracle
    };
    assert!(matches!(
        only_full.submit(&request()).await,
        Err(ReviewError::Provider { status: 429, .. })
    ));
}
