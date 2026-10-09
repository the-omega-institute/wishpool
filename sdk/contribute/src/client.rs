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
