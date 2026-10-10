//! A small blocking client for the wishpool contribution API.

use serde_json::{Value, json};

pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
    token: String,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub detail: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.detail, self.status)
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

impl Client {
    /// `base` is the venue origin, e.g. `https://wishpool.example.org`;
    /// `token` a NyxID access token (or `dev:<name>` against a dev server).
    pub fn new(base: &str, token: &str) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .user_agent("wishpool-contribute/0.1")
            .build()
            .expect("HTTP client");
        Self {
            http,
            base: format!("{}/api/v1", base.trim_end_matches('/')),
            token: token.to_owned(),
        }
    }

    fn send(&self, request: reqwest::blocking::RequestBuilder) -> ApiResult<Value> {
        let response = request
            .bearer_auth(&self.token)
            .send()
            .map_err(|e| ApiError {
                status: 0,
                detail: e.to_string(),
            })?;
        let status = response.status().as_u16();
        let text = response.text().unwrap_or_default();
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if (200..300).contains(&status) {
            Ok(body)
        } else {
            let detail = body["detail"].as_str().map(str::to_owned).unwrap_or(text);
            Err(ApiError { status, detail })
        }
    }

    pub fn get(&self, path: &str) -> ApiResult<Value> {
        self.send(self.http.get(format!("{}{path}", self.base)))
    }

    pub fn post(&self, path: &str, body: &Value) -> ApiResult<Value> {
        self.send(self.http.post(format!("{}{path}", self.base)).json(body))
    }

    pub fn delete(&self, path: &str) -> ApiResult<Value> {
        self.send(self.http.delete(format!("{}{path}", self.base)))
    }

    pub fn conjectures(&self) -> ApiResult<Value> {
        self.get("/conjectures?limit=100")
    }
    pub fn target(&self, record: &str, claim: &str) -> ApiResult<Value> {
        self.get(&format!("/conjectures/{record}/{claim}"))
    }
    pub fn attempt(
        &self,
        record: &str,
        claim: &str,
        solution: &str,
        agent: Option<&str>,
    ) -> ApiResult<Value> {
        self.post(
            &format!("/conjectures/{record}/{claim}/attempts"),
            &json!({"solution": solution, "as_agent": agent}),
        )
    }
    pub fn attempt_status(&self, id: &str) -> ApiResult<Value> {
        self.get(&format!("/attempts/{id}"))
    }

    pub fn open_tasks(&self, kind: Option<&str>, limit: u32) -> ApiResult<Value> {
        let mut path = format!("/tasks?status=open&limit={limit}");
        if let Some(kind) = kind {
            path.push_str(&format!("&kind={kind}"));
        }
        self.get(&path)
    }

    pub fn lease(&self, task: &str) -> ApiResult<Value> {
        self.post(&format!("/tasks/{task}/lease"), &json!({}))
    }

    pub fn release(&self, task: &str) -> ApiResult<Value> {
        self.delete(&format!("/tasks/{task}/lease"))
    }

    /// The task plus what an agent needs to do it: the statement, the
    /// statements it depends on, the paper's title and abstract, and the
    /// rules for its kind.
    pub fn context(&self, task: &str) -> ApiResult<Value> {
        let mut context = self.get(&format!("/tasks/{task}"))?;
        let kind = context["task"]["kind"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        context["rules"] = json!(crate::rules::for_kind(&kind));
        Ok(context)
    }

    pub fn submit(&self, task: &str, contribution: &Value) -> ApiResult<Value> {
        self.post(&format!("/tasks/{task}/contributions"), contribution)
    }
}

#[cfg(test)]
mod solving_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    #[test]
    fn solving_client_downloads_exact_target_and_submits_without_claiming_verification() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for (method, path, body) in [
                (
                    "GET",
                    "/api/v1/conjectures?limit=100",
                    json!({"items": [], "next_before": null}),
                ),
                (
                    "GET",
                    "/api/v1/conjectures/WP-2026-0001/C1",
                    json!({"target": {"lean":"import Mathlib\ndef wishpool_target_prop : Prop := True\n", "digest":"exact-digest"}}),
                ),
                (
                    "POST",
                    "/api/v1/conjectures/WP-2026-0001/C1/attempts",
                    json!({"id":"a1", "state":"queued"}),
                ),
                (
                    "GET",
                    "/api/v1/attempts/a1",
                    json!({"id":"a1", "state":"rejected", "receipt":{"reason":"Wrong target type"}}),
                ),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut buf = [0; 4096];
                    let count = stream.read(&mut buf).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buf[..count]);
                    if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        break pos + 4;
                    }
                };
                let header = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                assert!(header.starts_with(&format!("{method} {path} HTTP/1.1")));
                assert!(
                    header
                        .to_lowercase()
                        .contains("authorization: bearer test-token")
                );
                let length = header
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|n| n.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                while bytes.len() < header_end + length {
                    let mut buf = [0; 4096];
                    let count = stream.read(&mut buf).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buf[..count]);
                }
                if method == "POST" {
                    let submitted: Value = serde_json::from_slice(&bytes[header_end..]).unwrap();
                    assert_eq!(
                        submitted,
                        json!({"solution":"import Target\nproof", "as_agent":"Proof agent"})
                    );
                    assert!(submitted.get("receipt").is_none());
                }
                let text = body.to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len()).unwrap();
            }
        });
        let client = Client::new(&origin, "test-token");
        assert_eq!(client.conjectures().unwrap()["items"], json!([]));
        let target = client.target("WP-2026-0001", "C1").unwrap();
        assert_eq!(target["target"]["digest"], "exact-digest");
        assert!(
            target["target"]["lean"]
                .as_str()
                .unwrap()
                .ends_with("True\n")
        );
        assert_eq!(
            client
                .attempt(
                    "WP-2026-0001",
                    "C1",
                    "import Target\nproof",
                    Some("Proof agent")
                )
                .unwrap()["state"],
            "queued"
        );
        assert_eq!(
            client.attempt_status("a1").unwrap()["receipt"]["reason"],
            "Wrong target type"
        );
        server.join().unwrap();
    }
}
