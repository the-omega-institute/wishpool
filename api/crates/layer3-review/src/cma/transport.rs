use super::{identifier, invalid};
use crate::{ReviewError, ReviewResult, oracle::local_command};
use async_trait::async_trait;
use serde_json::Value;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::io::AsyncReadExt;
const MAX_WIRE: usize = 12_000_000;

#[async_trait]
pub trait Transport: Send + Sync {
    async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        key: Option<&str>,
    ) -> ReviewResult<TransportReply>;
}
pub struct TransportReply {
    pub status: u16,
    pub body: String,
    pub response_id: Option<String>,
}
impl TransportReply {
    pub fn json(self) -> ReviewResult<Value> {
        check(self.status, &self.body)?;
        if self.body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&self.body).map_err(|_| invalid("invalid CMA JSON envelope"))
    }
    pub fn response_id(self) -> ReviewResult<String> {
        check(self.status, &self.body)?;
        if let Some(id) = self.response_id {
            return identifier(&serde_json::json!({"id":id}), "id");
        }
        for frame in self.body.split("\n\n") {
            for line in frame.lines() {
                if let Some(data) = line.strip_prefix("data:")
                    && let Ok(v) = serde_json::from_str::<Value>(data.trim())
                    && v["type"] == "response.created"
                {
                    return identifier(&v["response"], "id");
                }
            }
        }
        Err(invalid("CMA stream has no response receipt"))
    }
}
fn check(status: u16, text: &str) -> ReviewResult<()> {
    let v = serde_json::from_str::<Value>(text).unwrap_or(Value::Null);
    if (200..300).contains(&status) && v.get("error").is_none() {
        return Ok(());
    }
    let code = v["error"]["code"]
        .as_str()
        .or(v["code"].as_str())
        .or(v["error"].as_str())
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 80
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
        .unwrap_or("cma_request_failed");
    Err(ReviewError::Provider {
        status: if (200..300).contains(&status) {
            503
        } else {
            status
        },
        body: code.into(),
    })
}

#[async_trait]
pub trait TokenSource: Send + Sync {
    async fn token(&self) -> ReviewResult<String>;
}
pub struct HttpTransport {
    pub base_url: String,
    pub tokens: Arc<dyn TokenSource>,
}
#[async_trait]
impl Transport for HttpTransport {
    async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        key: Option<&str>,
    ) -> ReviewResult<TransportReply> {
        let url = format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ReviewError::Transport("CMA HTTP client unavailable".into()))?;
        let token = self.tokens.token().await?;
        if token.trim().is_empty() || token.trim().starts_with("nyxid_ag_") {
            return Err(invalid("CMA requires a NyxID user access token"));
        }
        let mut request = client
            .request(
                reqwest::Method::from_bytes(method.as_bytes())
                    .map_err(|_| invalid("invalid CMA method"))?,
                url,
            )
            .bearer_auth(token.trim());
        let streaming = body.as_ref().is_some_and(|b| b["stream"] == true);
        if let Some(body) = body {
            if serde_json::to_vec(&body)
                .map_err(|_| invalid("invalid CMA body"))?
                .len()
                > 2 * 1024 * 1024
            {
                return Err(invalid("CMA body exceeds 2 MiB"));
            }
            request = request.json(&body);
        }
        if let Some(key) = key {
            request = request.header("Idempotency-Key", key);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| ReviewError::Transport("CMA HTTP request failed".into()))?;
        let status = response.status().as_u16();
        let id = response
            .headers()
            .get("x-cma-response-id")
            .and_then(|h| h.to_str().ok())
            .map(str::to_owned);
        // The handshake is the durable receipt. Dropping its stream leaves execution running.
        if (200..300).contains(&status) && id.is_some() {
            return Ok(TransportReply {
                status,
                body: String::new(),
                response_id: id,
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| ReviewError::Transport("CMA HTTP read failed".into()))?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_WIRE {
                return Err(invalid("CMA envelope too large"));
            }
            bytes.extend_from_slice(&chunk);
            if streaming && (200..300).contains(&status) {
                let receipt = TransportReply {
                    status,
                    body: String::from_utf8_lossy(&bytes).replace("\r\n", "\n"),
                    response_id: None,
                };
                if let Ok(id) = receipt.response_id() {
                    return Ok(TransportReply {
                        status,
                        body: String::new(),
                        response_id: Some(id),
                    });
                }
            }
        }
        let reply = TransportReply {
            status,
            body: String::from_utf8(bytes).map_err(|_| invalid("CMA envelope is not UTF-8"))?,
            response_id: None,
        };
        check(reply.status, &reply.body)?;
        Ok(reply)
    }
}

pub struct CliTransport {
    pub program: PathBuf,
    pub work_dir: PathBuf,
}
#[async_trait]
impl Transport for CliTransport {
    async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        key: Option<&str>,
    ) -> ReviewResult<TransportReply> {
        std::fs::create_dir_all(&self.work_dir)
            .map_err(|_| invalid("CMA CLI scratch unavailable"))?;
        let work = tempfile::tempdir_in(&self.work_dir)
            .map_err(|_| invalid("CMA CLI scratch unavailable"))?;
        let mut command = local_command(&self.program);
        command.args([
            "proxy", "request", "cma", path, "--method", method, "--output", "json",
        ]);
        if let Some(key) = key {
            command.arg("-H").arg(format!("Idempotency-Key:{key}"));
        }
        let streaming = body.as_ref().is_some_and(|b| b["stream"] == true);
        if streaming {
            command.arg("--stream");
        }
        if let Some(body) = body {
            let bytes = serde_json::to_vec(&body).map_err(|_| invalid("invalid CMA body"))?;
            if bytes.len() > 2 * 1024 * 1024 {
                return Err(invalid("CMA body exceeds 2 MiB"));
            }
            let path = work.path().join("request.json");
            std::fs::write(&path, bytes).map_err(|_| invalid("CMA CLI input unavailable"))?;
            command
                .args(["-H", "Content-Type:application/json", "--data"])
                .arg(format!("@{}", path.display()));
        }
        let mut child = command
            .spawn()
            .map_err(|_| ReviewError::Transport("CMA CLI could not start".into()))?;
        let mut stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let read = async {
            let read_out = async {
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 8192];
                loop {
                    let n = stdout
                        .read(&mut buffer)
                        .await
                        .map_err(|_| invalid("CMA CLI output unavailable"))?;
                    if n == 0 {
                        break;
                    }
                    if bytes.len() + n > MAX_WIRE {
                        return Err(invalid("CMA CLI output too large"));
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                    if streaming {
                        let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                        let reply = TransportReply {
                            status: 200,
                            body: text,
                            response_id: None,
                        };
                        if let Ok(id) = reply.response_id() {
                            let _ = child.kill().await;
                            return Ok((bytes, Some(id)));
                        }
                    }
                }
                Ok((bytes, None))
            };
            // Read stderr concurrently, bounded; never include it in diagnostics.
            let read_err = async {
                let mut bytes = Vec::new();
                stderr
                    .take(65537)
                    .read_to_end(&mut bytes)
                    .await
                    .map(|_| bytes)
            };
            let (out, err) = tokio::join!(read_out, read_err);
            let (out, id) = out?;
            let err = err.map_err(|_| invalid("CMA CLI status unavailable"))?;
            let exit = child
                .wait()
                .await
                .map_err(|_| invalid("CMA CLI status unavailable"))?;
            parse_cli(&out, &err, exit.success() || id.is_some(), id)
        };
        tokio::time::timeout(Duration::from_secs(120), read)
            .await
            .map_err(|_| ReviewError::Transport("CMA CLI request timed out".into()))?
    }
}
pub(super) fn parse_cli(
    out: &[u8],
    err: &[u8],
    success: bool,
    id: Option<String>,
) -> ReviewResult<TransportReply> {
    let stderr = String::from_utf8_lossy(err);
    let status = stderr.lines().find_map(|line| {
        line.split_once("HTTP ")?
            .1
            .split_whitespace()
            .next()?
            .parse::<u16>()
            .ok()
            .filter(|n| (100..600).contains(n))
    });
    let body = std::str::from_utf8(out)
        .map_err(|_| invalid("CMA CLI output is not UTF-8"))?
        .replace("\r\n", "\n");
    let reply = TransportReply {
        status: status.unwrap_or(if success { 200 } else { 503 }),
        body,
        response_id: id,
    };
    check(reply.status, &reply.body)?;
    Ok(reply)
}
