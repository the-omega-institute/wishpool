use axum::{Json, Router, http::HeaderMap, routing::post};
use serde_json::{Value, json};

use super::*;

async fn serve(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{address}/v1")
}

fn document() -> Document {
    Document {
        title: "T".into(),
        abstract_text: "A".into(),
        text: Some("Theorem 1.".into()),
    }
}

fn statement() -> Statement {
    Statement {
        id: "C1".into(),
        label: "Theorem 1".into(),
        statement: "x".into(),
        main: true,
        proved: true,
        depends_on: vec![],
    }
}

#[tokio::test]
async fn sends_bearer_and_parses_fenced_json() {
    let router = Router::new().route(
        "/v1/chat/completions",
        post(|headers: HeaderMap, Json(body): Json<Value>| async move {
            assert_eq!(headers["authorization"], "Bearer nyx-token");
            assert_eq!(body["model"], "claude-test");
            assert_eq!(body["messages"][0]["role"], "system");
            let content = "```json\n{\"assessments\":[{\"claim\":\"C1\",\"content\":true,\"witnesses\":[\"Lemma 2\"],\"rationale\":\"r\"}]}\n```";
            Json(json!({
                "choices": [ { "message": { "content": content } } ],
                "usage": { "prompt_tokens": 120, "completion_tokens": 30 }
            }))
        }),
    );
    let base = serve(router).await;
    let model = ChatModel::new(&base, "nyx-token".into(), "claude-test".into()).unwrap();
    let (proposals, usage) = model
        .assess_escape(&document(), &[statement()])
        .await
        .unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].claim, "C1");
    assert!(proposals[0].content);
    assert_eq!(
        usage,
        Usage {
            input: 120,
            output: 30
        }
    );
}

#[tokio::test]
async fn provider_errors_are_reported_with_status() {
    let router = Router::new().route(
        "/v1/chat/completions",
        post(|| async { (axum::http::StatusCode::PAYMENT_REQUIRED, "no credit") }),
    );
    let base = serve(router).await;
    let model = ChatModel::new(&base, "t".into(), "m".into()).unwrap();
    match model.judge_statement("x", "").await {
        Err(ReviewError::Provider { status, body }) => {
            assert_eq!(status, 402);
            assert_eq!(body, "no credit");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn rejects_replies_without_an_object() {
    assert!(parse_json_object::<Value>("no json here").is_err());
    assert!(parse_json_object::<Value>("} {").is_err());
}

#[tokio::test]
async fn search_queries_are_cleaned_deduplicated_and_capped() {
    let router = Router::new().route(
        "/v1/chat/completions",
        post(|| async {
            let content = r#"{"queries":["nested  recurrence","$\\rep$ automatic","nested recurrence","Hofstadter","Walnut",""]}"#;
            Json(json!({ "choices": [ { "message": { "content": content } } ] }))
        }),
    );
    let base = serve(router).await;
    let model = ChatModel::new(&base, "t".into(), "m".into()).unwrap();
    let (queries, _) = model.search_queries("T", "A", "x").await.unwrap();
    assert_eq!(
        queries,
        ["nested recurrence", "rep automatic", "Hofstadter"]
    );
}
