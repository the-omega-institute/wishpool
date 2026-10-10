//! Durable external tasks through the standalone Oracle broker via NyxID's proxy.

use std::{path::PathBuf, process::Stdio, time::Duration};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::process::Command;

use crate::{ReviewError, ReviewResult};

pub struct OracleRequest {
    pub prompt: String,
    pub pdf: Option<(String, Vec<u8>)>,
    pub client_ref: String,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleSubmitted {
    pub task: String,
    pub queue_position: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleStatus {
    Queued {
        position: Option<u32>,
    },
    Running,
    Completed {
        text: String,
        model: Option<String>,
    },
    Failed {
        reason: String,
        detail: Option<String>,
    },
    Cancelled,
}

/// One request is submitted to every configured pool; the first completed
/// answer wins and the other tasks are cancelled. The task handle joins the
/// pools' task ids with commas. Each pool applies its own default model
/// label, so no model hint is sent.
#[async_trait]
pub trait Oracle: Send + Sync {
    fn engine(&self) -> &str;
    async fn submit(&self, request: &OracleRequest) -> ReviewResult<OracleSubmitted>;
    async fn poll(&self, task: &str) -> ReviewResult<OracleStatus>;
}

/// One pool's operations; the race over pools is shared.
#[async_trait]
trait Pools: Send + Sync {
    fn pools(&self) -> &[String];
    async fn submit_to(&self, pool: &str, request: &OracleRequest)
    -> ReviewResult<OracleSubmitted>;
    async fn poll_one(&self, task: &str) -> ReviewResult<OracleStatus>;
    async fn cancel_one(&self, task: &str) -> ReviewResult<()>;
}

async fn race_submit(oracle: &dyn Pools, request: &OracleRequest) -> ReviewResult<OracleSubmitted> {
    validate(request)?;
    let mut tasks = Vec::new();
    let mut position: Option<u32> = None;
    let mut first_error = None;
    for pool in oracle.pools() {
        // Idempotency keys are submitter-scoped, so each pool gets its own.
        let scoped = OracleRequest {
            prompt: request.prompt.clone(),
            pdf: request.pdf.clone(),
            client_ref: format!("{}:{pool}", request.client_ref),
            tag: request.tag.clone(),
        };
        match oracle.submit_to(pool, &scoped).await {
            Ok(submitted) => {
                tasks.push(submitted.task);
                position = match (position, submitted.queue_position) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
            }
            Err(error) => {
                tracing::warn!(pool, %error, "oracle pool refused the task");
                first_error.get_or_insert(error);
            }
        }
    }
    match first_error {
        Some(error) if tasks.is_empty() => Err(error),
        _ => Ok(OracleSubmitted {
            task: tasks.join(","),
            queue_position: position,
        }),
    }
}

async fn race_poll(oracle: &dyn Pools, handle: &str) -> ReviewResult<OracleStatus> {
    let tasks: Vec<&str> = handle.split(',').filter(|t| !t.is_empty()).collect();
    let mut statuses = Vec::new();
    let mut first_error = None;
    for task in &tasks {
        match oracle.poll_one(task).await {
            Ok(completed @ OracleStatus::Completed { .. }) => {
                for other in tasks.iter().filter(|t| *t != task) {
                    if let Err(error) = oracle.cancel_one(other).await {
                        tracing::warn!(task = other, %error, "losing oracle task was not cancelled");
                    }
                }
                return Ok(completed);
            }
            Ok(status) => statuses.push(status),
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    if statuses.is_empty() {
        return Err(first_error
            .unwrap_or_else(|| ReviewError::Output("oracle task handle is empty".into())));
    }
    if statuses.contains(&OracleStatus::Running) {
        return Ok(OracleStatus::Running);
    }
    let queued = statuses.iter().filter_map(|s| match s {
        OracleStatus::Queued { position } => Some(*position),
        _ => None,
    });
    if let Some(position) = queued.reduce(|a, b| match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }) {
        return Ok(OracleStatus::Queued { position });
    }
    // A pool that could not be polled may still answer.
    if let Some(error) = first_error {
        return Err(error);
    }
    Ok(statuses
        .into_iter()
        .find(|s| matches!(s, OracleStatus::Failed { .. }))
        .unwrap_or(OracleStatus::Cancelled))
}

/// The HTTP API and proxy CLI return the same broker task object.
#[derive(Debug, Deserialize, Serialize)]
pub struct TaskJson {
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub queue_position: Option<u32>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    response: Option<String>,
    #[serde(default)]
    failure_reason: Option<String>,
    #[serde(default)]
    failure_detail: Option<String>,
    #[serde(default)]
    observed_model_switcher: Option<String>,
    #[serde(default)]
    observed_model_effort: Option<String>,
}

pub fn parse_task(text: &str) -> ReviewResult<TaskJson> {
    parse_response(text, None)
}

fn parse_response(text: &str, status: Option<u16>) -> ReviewResult<TaskJson> {
    let value = serde_json::from_str::<serde_json::Value>(text);
    if status.is_some_and(|s| !(200..300).contains(&s))
        || value.as_ref().is_ok_and(|v| v.get("error").is_some())
    {
        // Only retain a short code, never the body or message (which may echo
        // credentials, prompts or attachments).
        let code = value
            .as_ref()
            .ok()
            .and_then(|v| v.get("error"))
            .and_then(|v| v.as_str())
            .filter(|s| {
                !s.is_empty()
                    && s.len() <= 64
                    && s.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            })
            .unwrap_or("oracle_request_failed");
        return Err(match status {
            Some(status)
                if !(200..300).contains(&status)
                    && status != 429
                    && !(500..600).contains(&status) =>
            {
                ReviewError::Provider {
                    status,
                    body: code.into(),
                }
            }
            _ => ReviewError::Transport(format!("oracle request failed ({status:?}): {code}")),
        });
    }
    let value = value.map_err(|_| ReviewError::Output("oracle task JSON is invalid".into()))?;
    serde_json::from_value(value)
        .map_err(|_| ReviewError::Output("oracle task fields are invalid".into()))
}

fn observed_model(switcher: Option<String>, effort: Option<String>) -> Option<String> {
    let label = |value: String| {
        let value = value.trim();
        (!value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)))
        .then(|| value.to_string())
    };
    let model = switcher.and_then(label)?;
    Some(match effort.and_then(label) {
        Some(effort) => format!("{model} · {effort}"),
        None => model,
    })
}

impl TaskJson {
    fn submitted(self) -> ReviewResult<OracleSubmitted> {
        if self.task_id.trim().is_empty() {
            return Err(ReviewError::Output(
                "oracle submission has no task_id".into(),
            ));
        }
        Ok(OracleSubmitted {
            task: self.task_id,
            queue_position: self.queue_position,
        })
    }

    pub fn status(self) -> ReviewResult<OracleStatus> {
        Ok(match self.status.as_str() {
            "queued" => OracleStatus::Queued {
                position: self.queue_position,
            },
            "dispatched" | "running" => OracleStatus::Running,
            "completed" => OracleStatus::Completed {
                text: self.response.ok_or_else(|| {
                    ReviewError::Output("completed oracle task has no response text".into())
                })?,
                model: observed_model(self.observed_model_switcher, self.observed_model_effort),
            },
            "failed" => OracleStatus::Failed {
                reason: self.failure_reason.unwrap_or_else(|| "unknown".into()),
                detail: self.failure_detail,
            },
            "cancelled" => OracleStatus::Cancelled,
            other => {
                return Err(ReviewError::Output(format!(
                    "unknown oracle status {other:?}"
                )));
            }
        })
    }
}

fn validate(request: &OracleRequest) -> ReviewResult<()> {
    if request.prompt.chars().count() > 500_000 {
        return Err(ReviewError::Output(
            "oracle prompt exceeds 500,000 characters".into(),
        ));
    }
    if request
        .pdf
        .as_ref()
        .is_some_and(|(_, bytes)| bytes.len().div_ceil(3).saturating_mul(4) > 12_000_000)
    {
        return Err(ReviewError::Output(
            "oracle PDF base64 exceeds 12,000,000 bytes".into(),
        ));
    }
    Ok(())
}

fn request_body(request: &OracleRequest) -> serde_json::Value {
    let mut body =
        json!({ "prompt": request.prompt, "client_ref": request.client_ref, "tag": request.tag });
    if let Some((name, bytes)) = &request.pdf {
        body["pdf_name"] = json!(name);
        body["pdf_base64"] = json!(STANDARD.encode(bytes));
    }
    body
}

pub struct OracleHttp {
    pub base_url: String,
    pub token: String,
    pub pools: Vec<String>,
}

impl OracleHttp {
    async fn request(
        &self,
        path: &[&str],
        body: Option<serde_json::Value>,
    ) -> ReviewResult<TaskJson> {
        let mut url =
            reqwest::Url::parse(&self.base_url).map_err(|e| ReviewError::Output(e.to_string()))?;
        url.path_segments_mut()
            .map_err(|_| ReviewError::Output("oracle base URL cannot hold paths".into()))?
            .pop_if_empty()
            .extend(["api", "v1", "oracle"])
            .extend(path);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent(concat!("wishpool/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| ReviewError::Transport(e.to_string()))?;
        let request = match body {
            Some(body) => client.post(url).json(&body),
            None => client.get(url),
        }
        .bearer_auth(&self.token);
        let response = request
            .send()
            .await
            .map_err(|e| ReviewError::Transport(e.to_string()))?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|e| ReviewError::Transport(e.to_string()))?;
        parse_response(&text, Some(status))
    }
}

#[async_trait]
impl Oracle for OracleHttp {
    fn engine(&self) -> &str {
        "nyxid-oracle"
    }

    async fn submit(&self, request: &OracleRequest) -> ReviewResult<OracleSubmitted> {
        race_submit(self, request).await
    }

    async fn poll(&self, task: &str) -> ReviewResult<OracleStatus> {
        race_poll(self, task).await
    }
}

#[async_trait]
impl Pools for OracleHttp {
    fn pools(&self) -> &[String] {
        &self.pools
    }

    async fn submit_to(
        &self,
        pool: &str,
        request: &OracleRequest,
    ) -> ReviewResult<OracleSubmitted> {
        self.request(&["pools", pool, "tasks"], Some(request_body(request)))
            .await?
            .submitted()
    }

    async fn poll_one(&self, task: &str) -> ReviewResult<OracleStatus> {
        self.request(&["tasks", task], None).await?.status()
    }

    async fn cancel_one(&self, task: &str) -> ReviewResult<()> {
        match self
            .request(&["tasks", task, "cancel"], Some(json!({})))
            .await
        {
            Ok(_)
            | Err(ReviewError::Provider {
                status: 404 | 409, ..
            }) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

pub struct OracleCli {
    pub program: PathBuf,
    pub pools: Vec<String>,
    pub work_dir: PathBuf,
}

/// CLI credentials are found only in the operator's home; deployment secrets
/// never enter child environments.
pub(crate) fn local_command(program: &std::path::Path) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for name in ["PATH", "HOME"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

async fn run(mut command: Command) -> ReviewResult<TaskJson> {
    let output = tokio::time::timeout(Duration::from_secs(120), command.output())
        .await
        .map_err(|_| ReviewError::Transport("oracle CLI timed out".into()))?
        .map_err(|e| ReviewError::Transport(format!("oracle CLI: {e}")))?;
    if !output.status.success() {
        return Err(ReviewError::Transport(format!(
            "oracle CLI exited {}",
            output.status
        )));
    }
    // The proxy CLI exits successfully even on HTTP failures.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let status = stderr.lines().find_map(|line| {
        let (_, rest) = line.split_once("HTTP ")?;
        rest.split_whitespace()
            .next()?
            .parse::<u16>()
            .ok()
            .filter(|s| (100..600).contains(s))
    });
    parse_response(
        std::str::from_utf8(&output.stdout)
            .map_err(|_| ReviewError::Output("oracle CLI output is not UTF-8".into()))?,
        status,
    )
}

impl OracleCli {
    async fn request(&self, path: &str, body: Option<serde_json::Value>) -> ReviewResult<TaskJson> {
        let mut command = local_command(&self.program);
        command.args(["proxy", "request", "oracle", path]);
        // Keep the directory alive until the child exits, including failures.
        let _work = if let Some(body) = body {
            std::fs::create_dir_all(&self.work_dir)
                .map_err(|e| ReviewError::Transport(e.to_string()))?;
            let mut directories = tempfile::Builder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                directories.permissions(std::fs::Permissions::from_mode(0o700));
            }
            let work = directories
                .tempdir_in(&self.work_dir)
                .map_err(|e| ReviewError::Transport(e.to_string()))?;
            let path = work.path().join("request.json");
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options
                .open(&path)
                .map_err(|e| ReviewError::Transport(e.to_string()))?;
            serde_json::to_writer(file, &body)
                .map_err(|e| ReviewError::Transport(e.to_string()))?;
            command
                .args(["--method", "POST", "--data"])
                .arg(format!("@{}", path.display()));
            Some(work)
        } else {
            None
        };
        command.args(["--output", "json"]);
        run(command).await
    }
}

#[async_trait]
impl Oracle for OracleCli {
    fn engine(&self) -> &str {
        "nyxid-oracle"
    }

    async fn submit(&self, request: &OracleRequest) -> ReviewResult<OracleSubmitted> {
        race_submit(self, request).await
    }

    async fn poll(&self, task: &str) -> ReviewResult<OracleStatus> {
        race_poll(self, task).await
    }
}

#[async_trait]
impl Pools for OracleCli {
    fn pools(&self) -> &[String] {
        &self.pools
    }

    async fn submit_to(
        &self,
        pool: &str,
        request: &OracleRequest,
    ) -> ReviewResult<OracleSubmitted> {
        self.request(
            &format!("api/v1/oracle/pools/{pool}/tasks"),
            Some(request_body(request)),
        )
        .await?
        .submitted()
    }

    async fn poll_one(&self, task: &str) -> ReviewResult<OracleStatus> {
        self.request(&format!("api/v1/oracle/tasks/{task}"), None)
            .await?
            .status()
    }

    async fn cancel_one(&self, task: &str) -> ReviewResult<()> {
        match self
            .request(
                &format!("api/v1/oracle/tasks/{task}/cancel"),
                Some(json!({})),
            )
            .await
        {
            Ok(_)
            | Err(ReviewError::Provider {
                status: 404 | 409, ..
            }) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests;
