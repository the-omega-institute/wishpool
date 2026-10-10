//! A review model behind an OpenAI-compatible chat-completions endpoint.
//!
//! In production the base URL is the NyxID LLM gateway
//! (`<nyxid>/api/v1/llm/gateway/v1`), which selects the provider from the
//! model name and injects the account's provider credential; this crate only
//! ever holds a NyxID token.

mod prompts;

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;

use crate::{
    Document, EscapeProposal, JudgementDraft, ReviewError, ReviewModel, ReviewResult, Statement,
    Usage,
};

/// Characters of document text included in one prompt.
pub const PROMPT_TEXT_CHARS: usize = 60_000;

pub struct ChatModel {
    client: reqwest::Client,
    endpoint: String,
    token: String,
    model: String,
}

impl ChatModel {
    /// `base_url` is the API root that `chat/completions` is appended to.
    pub fn new(base_url: &str, token: String, model: String) -> ReviewResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| ReviewError::Transport(e.to_string()))?;
        let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        Ok(Self {
            client,
            endpoint,
            token,
            model,
        })
    }

    /// A completion and the usage the endpoint reported for it.
    pub(crate) async fn complete_metered<T: DeserializeOwned>(
        &self,
        system: &str,
        user: String,
    ) -> ReviewResult<(T, Usage)> {
        let body = json!({
            "model": self.model,
            "temperature": 0,
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
        });
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| ReviewError::Transport(e.to_string()))?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|e| ReviewError::Transport(e.to_string()))?;
        if !(200..300).contains(&status) {
            return Err(ReviewError::Provider {
                status,
                body: text.chars().take(2_000).collect(),
            });
        }
        let completion: Completion = serde_json::from_str(&text)
            .map_err(|e| ReviewError::Output(format!("completion envelope: {e}")))?;
        let usage = completion.usage.unwrap_or_default();
        let content = completion
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| ReviewError::Output("empty completion".into()))?;
        Ok((
            crate::referee_prompts::parse_answer(&content)?,
            Usage {
                input: usage.prompt_tokens,
                output: usage.completion_tokens,
            },
        ))
    }
}

#[derive(Deserialize, Default)]
struct UsageEnvelope {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

#[derive(Deserialize)]
struct Completion {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<UsageEnvelope>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

/// Parse the first JSON object in a model's reply, tolerating code fences.
#[cfg(test)]
pub(crate) fn parse_json_object<T: DeserializeOwned>(content: &str) -> ReviewResult<T> {
    let start = content
        .find('{')
        .ok_or_else(|| ReviewError::Output("no JSON object in reply".into()))?;
    let end = content
        .rfind('}')
        .ok_or_else(|| ReviewError::Output("unterminated JSON object".into()))?;
    if end < start {
        return Err(ReviewError::Output("malformed JSON object".into()));
    }
    serde_json::from_str(&content[start..=end]).map_err(|e| ReviewError::Output(e.to_string()))
}

fn document_block(document: &Document) -> String {
    let text = document
        .text
        .as_deref()
        .map(|t| t.chars().take(PROMPT_TEXT_CHARS).collect::<String>())
        .unwrap_or_else(|| "(full text unavailable; review the abstract only)".into());
    format!(
        "TITLE: {}\n\nABSTRACT:\n{}\n\nTEXT:\n{}",
        document.title, document.abstract_text, text
    )
}

fn statements_block(statements: &[Statement]) -> String {
    serde_json::to_string_pretty(statements).unwrap_or_default()
}

#[derive(Deserialize)]
struct EscapeReply {
    assessments: Vec<EscapeProposal>,
}

#[async_trait]
impl ReviewModel for ChatModel {
    fn engine(&self) -> &str {
        "openai-compatible"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn assess_escape(
        &self,
        document: &Document,
        statements: &[Statement],
    ) -> ReviewResult<(Vec<EscapeProposal>, Usage)> {
        let user = format!(
            "{}\n\nCLAIMS:\n{}",
            document_block(document),
            statements_block(statements)
        );
        let (reply, usage): (EscapeReply, Usage) =
            self.complete_metered(prompts::ESCAPE, user).await?;
        Ok((reply.assessments, usage))
    }

    async fn judge_statement(
        &self,
        statement: &str,
        context: &str,
    ) -> ReviewResult<(JudgementDraft, Usage)> {
        let user = format!(
            "STATEMENT:\n{}\n\nDEPENDENCIES AND PAPER:\n{}",
            statement
                .chars()
                .take(PROMPT_TEXT_CHARS)
                .collect::<String>(),
            context.chars().take(20_000).collect::<String>()
        );
        self.complete_metered(prompts::STATEMENT, user).await
    }
}

#[cfg(test)]
mod tests;
