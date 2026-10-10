use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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
                model: None,
            },
        ),
        (
            r#"{"status":"completed","response":"answer","observed_model_switcher":"gpt_6","observed_model_effort":"pro"}"#,
            OracleStatus::Completed {
                text: "answer".into(),
                model: Some("gpt_6 · pro".into()),
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
                    assert_eq!(headers["user-agent"], concat!("wishpool/", env!("CARGO_PKG_VERSION")));
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
            get(|headers: HeaderMap| async move {
                assert_eq!(headers["user-agent"], concat!("wishpool/", env!("CARGO_PKG_VERSION")));
                Json(json!({"status":"completed","response":"report", "observed_model_switcher":"gpt_6", "observed_model_effort":"pro"}))
            }),
        );
    let oracle = OracleHttp {
        base_url: format!(
            "{}/api/v1/proxy/s/oracle/",
            serve(Router::new().nest("/api/v1/proxy/s/oracle", router)).await
        ),
        token,
        pools: vec!["test".into()],
    };
    assert_eq!(oracle.submit(&request()).await.unwrap().task, "t1");
    assert_eq!(
        oracle.poll("t1").await.unwrap(),
        OracleStatus::Completed {
            text: "report".into(),
            model: Some("gpt_6 · pro".into()),
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
        Err(ReviewError::Transport(_))
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
    let cancelled = Arc::new(AtomicUsize::new(0));
    let cancel_count = cancelled.clone();
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
        )
        .route(
            "/api/v1/oracle/tasks/ta/cancel",
            post(move |Json(body): Json<Value>| {
                let cancel_count = cancel_count.clone();
                async move {
                    assert_eq!(body, json!({}));
                    cancel_count.fetch_add(1, Ordering::SeqCst);
                    Json(json!({"status":"cancelled"}))
                }
            }),
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
            text: "report".into(),
            model: None,
        }
    );
    assert_eq!(cancelled.load(Ordering::SeqCst), 1);
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
        Err(ReviewError::Transport(_))
    ));
}

#[tokio::test]
async fn http_errors_are_short_and_capacity_failures_are_transient() {
    let router = Router::new().route(
        "/api/v1/oracle/tasks/{status}",
        get(
            |axum::extract::Path(status): axum::extract::Path<u16>| async move {
                (
                    StatusCode::from_u16(status).unwrap(),
                    Json(json!({
                        "error":"oracle_task_not_found", "error_code":11006,
                        "message":"private prompt/credential"
                    })),
                )
            },
        ),
    );
    let oracle = OracleHttp {
        base_url: serve(router).await,
        token: uuid_token(),
        pools: vec!["test".into()],
    };
    for status in [400, 401, 403, 404, 409, 429, 500, 503] {
        let error = oracle.poll_one(&status.to_string()).await.unwrap_err();
        assert!(!error.to_string().contains("private"));
        if status == 429 || status >= 500 {
            assert!(matches!(error, ReviewError::Transport(_)));
        } else {
            assert!(
                matches!(error, ReviewError::Provider { status: actual, body }
                if actual == status && body == "oracle_task_not_found")
            );
        }
    }
    // An error object remains an error even with a successful HTTP status.
    assert!(oracle.poll_one("200").await.is_err());
}

#[tokio::test]
async fn http_cancel_ignores_missing_or_settled_tasks() {
    let token = uuid_token();
    let expected_token = format!("Bearer {token}");
    let router = Router::new().route(
        "/api/v1/oracle/tasks/{status}/cancel",
        post(move |axum::extract::Path(status): axum::extract::Path<u16>, headers: HeaderMap, Json(body): Json<Value>| {
            let expected_token = expected_token.clone();
            async move {
                assert_eq!(body, json!({}));
                assert_eq!(headers["authorization"], expected_token);
                assert_eq!(headers["user-agent"], concat!("wishpool/", env!("CARGO_PKG_VERSION")));
                (StatusCode::from_u16(status).unwrap(), Json(if status == 200 {
                    json!({"status":"cancelled"})
                } else {
                    json!({"error":"oracle_cancel_failed", "message":"private prompt/credential"})
                }))
            }
        }),
    );
    let oracle = OracleHttp {
        base_url: serve(router).await,
        token,
        pools: vec![],
    };
    for status in [200, 404, 409] {
        oracle.cancel_one(&status.to_string()).await.unwrap();
    }
    assert!(matches!(
        oracle.cancel_one("500").await,
        Err(ReviewError::Transport(_))
    ));
    assert!(matches!(
        oracle.cancel_one("403").await,
        Err(ReviewError::Provider { status: 403, .. })
    ));
}

#[test]
fn provider_errors_and_observed_labels_are_sanitised() {
    for code in [
        json!("private prompt/credential"),
        json!("x".repeat(65)),
        json!({"secret":"private"}),
        Value::Null,
    ] {
        let text = json!({"error":code, "message":"private"}).to_string();
        assert!(
            matches!(parse_response(&text, Some(404)), Err(ReviewError::Provider { status:404, body }) if body == "oracle_request_failed")
        );
    }
    assert!(
        matches!(parse_response("private non-JSON body", Some(403)), Err(ReviewError::Provider { status:403, body }) if body == "oracle_request_failed")
    );
    assert!(matches!(
        parse_task(r#"{"error":"oracle_task_not_found"}"#),
        Err(ReviewError::Transport(_))
    ));
    assert_eq!(
        observed_model(Some(" gpt_6 ".into()), None),
        Some("gpt_6".into())
    );
    assert_eq!(
        observed_model(Some("gpt_6".into()), Some("bad\nlabel".into())),
        Some("gpt_6".into())
    );
    assert_eq!(
        observed_model(Some("bad\nlabel".into()), Some("pro".into())),
        None
    );
    assert_eq!(observed_model(None, Some("pro".into())), None);
}

#[cfg(unix)]
fn fake_cli(work: &std::path::Path) -> OracleCli {
    use std::os::unix::fs::PermissionsExt;
    let program = work.join("fake-nyxid");
    std::fs::write(&program, r#"#!/bin/sh
set -eu
[ "$1" = "proxy" ]
[ "$2" = "request" ]
[ "$3" = "oracle" ]
fixture=$(dirname "$0")
case "$4" in
  api/v1/oracle/pools/test-pool/tasks|api/v1/oracle/tasks/*/cancel)
    [ "$#" -eq 10 ]
    [ "$5" = "--method" ]
    [ "$6" = "POST" ]
    [ "$7" = "--data" ]
    [ "$9" = "--output" ]
    [ "${10}" = "json" ]
    case "$8" in @*) body=${8#@} ;; *) exit 21 ;; esac
    [ -f "$body" ]
    case "$(uname)" in
      Darwin) mode=$(stat -f '%Lp' "$body"); dir_mode=$(stat -f '%Lp' "$(dirname "$body")") ;;
      *) mode=$(stat -c '%a' "$body"); dir_mode=$(stat -c '%a' "$(dirname "$body")") ;;
    esac
    [ "$mode" = "600" ]
    [ "$dir_mode" = "700" ]
    printf '%s\n' "$body" >> "$fixture/body-paths"
    case "$4" in
      api/v1/oracle/pools/test-pool/tasks)
        cmp "$body" "$fixture/expected-body.json"
        if [ -f "$fixture/submit-error" ]; then
          status=$(cat "$fixture/submit-error")
          printf 'Proxy request failed (HTTP %s Test Error)\nprivate stderr/credential\n' "$status" >&2
          printf '%s' '{"error":"oracle_submit_failed","message":"private prompt/credential"}'
          exit 0
        fi
        if [ -f "$fixture/submitted" ]; then dedup=true; else dedup=false; fi
        touch "$fixture/submitted"
        printf '{"task_id":"task-1","status":"queued","attempts":0,"retry_count":0,"max_retries":3,"queue_position":2,"deduplicated":%s}' "$dedup"
        exit 0 ;;
      *) [ "$(cat "$body")" = '{}' ]; printf '%s\n' "$4" >> "$fixture/cancelled" ;;
    esac ;;
  api/v1/oracle/tasks/*)
    [ "$#" -eq 6 ]
    [ "$5" = "--output" ]
    [ "$6" = "json" ] ;;
  *) exit 22 ;;
esac
task=${4#api/v1/oracle/tasks/}
task=${task%/cancel}
case "$task" in
  queued) printf '%s' '{"status":"queued","queue_position":3}' ;;
  dispatched) printf '%s' '{"status":"dispatched","phase":"waiting_response"}' ;;
  completed) printf '%s' '{"status":"completed","response":"a full answer","observed_model_switcher":"gpt_6","observed_model_effort":"pro"}' ;;
  failed) printf '%s' '{"status":"failed","failure_reason":"model_unavailable","failure_detail":"page_crashed"}' ;;
  task-1|cancelled) printf '%s' '{"status":"cancelled"}' ;;
  404|409|429|500)
    printf 'Proxy request failed (HTTP %s Test Error)\nprivate stderr/credential\n' "$task" >&2
    printf '%s' '{"error":"oracle_task_not_found","error_code":11006,"message":"private prompt/credential"}' ;;
  no-status) printf '%s' '{"error":"oracle_task_not_found","message":"private prompt/credential"}' ;;
  exit-failure) printf 'private stderr/credential' >&2; exit 1 ;;
  *) exit 23 ;;
esac
"#).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(work.join("expected-body.json"), serde_json::to_vec(&json!({
        "prompt":"Read the paper", "client_ref":"wishpool:p:r1:v1:c1:test-pool", "tag":"wishpool", "pdf_name":"p.pdf", "pdf_base64":"JVBERg=="
    })).unwrap()).unwrap();
    OracleCli {
        program,
        pools: vec!["test-pool".into()],
        work_dir: work.join("requests"),
    }
}

#[cfg(unix)]
fn assert_body_files_removed(oracle: &OracleCli, expected: usize) {
    let paths =
        std::fs::read_to_string(oracle.program.parent().unwrap().join("body-paths")).unwrap();
    let paths: Vec<_> = paths.lines().collect();
    assert_eq!(paths.len(), expected);
    let unique: std::collections::HashSet<_> = paths.iter().collect();
    assert_eq!(unique.len(), expected, "every POST needs a fresh directory");
    for path in paths {
        assert!(!std::path::Path::new(path).exists());
        assert!(!std::path::Path::new(path).parent().unwrap().exists());
    }
    assert!(
        std::fs::read_dir(&oracle.work_dir)
            .unwrap()
            .next()
            .is_none()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cli_submits_and_deduplicates_with_private_body_files() {
    let work = tempfile::tempdir().unwrap();
    let oracle = fake_cli(work.path());
    let first = oracle.submit(&request()).await.unwrap();
    let second = oracle.submit(&request()).await.unwrap();
    assert_eq!(
        first,
        OracleSubmitted {
            task: "task-1".into(),
            queue_position: Some(2)
        }
    );
    assert_eq!(second, first);
    assert_body_files_removed(&oracle, 2);
}

#[cfg(unix)]
#[tokio::test]
async fn cli_polls_every_status_and_cancels_losing_tasks() {
    let work = tempfile::tempdir().unwrap();
    let oracle = fake_cli(work.path());
    for (task, expected) in [
        ("queued", OracleStatus::Queued { position: Some(3) }),
        ("dispatched", OracleStatus::Running),
        (
            "completed",
            OracleStatus::Completed {
                text: "a full answer".into(),
                model: Some("gpt_6 · pro".into()),
            },
        ),
        (
            "failed",
            OracleStatus::Failed {
                reason: "model_unavailable".into(),
                detail: Some("page_crashed".into()),
            },
        ),
        ("cancelled", OracleStatus::Cancelled),
    ] {
        assert_eq!(oracle.poll(task).await.unwrap(), expected);
    }
    assert!(matches!(
        oracle.poll("queued,completed").await.unwrap(),
        OracleStatus::Completed { .. }
    ));
    oracle.cancel_one("task-1").await.unwrap();
    assert_eq!(
        std::fs::read_to_string(work.path().join("cancelled")).unwrap(),
        "api/v1/oracle/tasks/queued/cancel\napi/v1/oracle/tasks/task-1/cancel\n"
    );
    assert_body_files_removed(&oracle, 2);
}

#[cfg(unix)]
#[tokio::test]
async fn cli_detects_exit_zero_errors_and_cleans_up_failed_cancels() {
    let work = tempfile::tempdir().unwrap();
    let oracle = fake_cli(work.path());
    for task in ["404", "429", "500", "no-status", "exit-failure"] {
        let error = oracle.poll(task).await.unwrap_err();
        assert!(!error.to_string().contains("private"));
        if task == "404" {
            assert!(
                matches!(error, ReviewError::Provider { status:404, body } if body == "oracle_task_not_found")
            );
        } else {
            assert!(matches!(error, ReviewError::Transport(_)));
        }
    }
    for task in ["404", "409"] {
        oracle.cancel_one(task).await.unwrap();
    }
    assert!(matches!(
        oracle.cancel_one("500").await,
        Err(ReviewError::Transport(_))
    ));
    assert!(matches!(
        oracle.cancel_one("exit-failure").await,
        Err(ReviewError::Transport(_))
    ));
    for status in [404, 429, 500] {
        std::fs::write(work.path().join("submit-error"), status.to_string()).unwrap();
        let error = oracle.submit(&request()).await.unwrap_err();
        assert!(!error.to_string().contains("private"));
        if status == 404 {
            assert!(
                matches!(error, ReviewError::Provider { status:404, body } if body == "oracle_submit_failed")
            );
        } else {
            assert!(matches!(error, ReviewError::Transport(_)));
        }
    }
    assert_body_files_removed(&oracle, 7);
}

#[cfg(unix)]
#[tokio::test]
async fn cli_keeps_large_pdf_and_prompt_off_argv_and_supports_no_attachment() {
    let work = tempfile::tempdir().unwrap();
    let oracle = fake_cli(work.path());
    let mut input = request();
    input.pdf = Some(("large.pdf".into(), vec![0; 9_000_000]));
    input.prompt = "é".repeat(500_000);
    for pdf in [input.pdf.clone(), None] {
        input.pdf = pdf;
        let mut expected = json!({
            "prompt":input.prompt, "client_ref":"wishpool:p:r1:v1:c1:test-pool", "tag":"wishpool"
        });
        if let Some((name, bytes)) = &input.pdf {
            expected["pdf_name"] = json!(name);
            expected["pdf_base64"] = json!(STANDARD.encode(bytes));
        }
        std::fs::write(
            work.path().join("expected-body.json"),
            serde_json::to_vec(&expected).unwrap(),
        )
        .unwrap();
        assert_eq!(oracle.submit(&input).await.unwrap().task, "task-1");
    }
    assert_body_files_removed(&oracle, 2);
}
