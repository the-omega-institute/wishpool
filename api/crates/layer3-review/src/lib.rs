//! Layer 3 of wishpool: the machine review adapter.
//!
//! The contract in this file is neutral: it names documents, statements,
//! escape proposals, judgement drafts and candidate relations, and nothing
//! about the domain's publication threshold. Provider-specific code lives in
//! its own module:
//!
//! - [`openai_compat`]: a chat-completions model, reached through the NyxID
//!   LLM gateway or any OpenAI-compatible endpoint.
//! - [`oracle`]: long-running referee tasks through the NyxID Oracle.
//! - [`advisor`]: advice and letter drafts from a local Codex workspace or a
//!   chat model.
//! - [`lean`]: formalization probes checked against a Lean 4 + Mathlib
//!   workspace.
//!
//! The binary maps these results onto Layer 2 judgements and reports.

pub mod advisor;
pub mod cma;
pub mod lean;
pub mod openai_compat;
pub mod oracle;
pub mod referee_prompts;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReviewError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("provider returned {status}: {body}")]
    Provider { status: u16, body: String },
    #[error("unusable model output: {0}")]
    Output(String),
}

pub type ReviewResult<T> = Result<T, ReviewError>;

/// The material a model reviews.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Document {
    pub title: String,
    pub abstract_text: String,
    /// The paper's LaTeX source with inputs inlined. Models review only what
    /// they are given.
    pub text: Option<String>,
}

/// One statement of the paper, as its author confirmed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    pub id: String,
    pub label: String,
    pub statement: String,
    pub main: bool,
    #[serde(default)]
    pub proved: bool,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscapeProposal {
    pub claim: String,
    /// True when the statement's proof carries a step that prior results do
    /// not give by instantiation, projection or normalisation alone.
    pub content: bool,
    #[serde(default)]
    pub witnesses: Vec<String>,
    pub rationale: String,
}

/// A model's judgement of one statement.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct JudgementDraft {
    /// `content` or `bind_only`.
    pub shape: String,
    #[serde(default)]
    pub witnesses: Vec<String>,
    pub rationale: String,
}

/// Token usage reported by the endpoint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

/// A model that drafts review findings. Every call returns the usage the
/// endpoint reported, so donated quota can be metered exactly.
#[async_trait]
pub trait ReviewModel: Send + Sync {
    /// Engine and model names recorded on every finding.
    fn engine(&self) -> &str;
    fn model(&self) -> &str;
    /// Judge every given statement of a paper in one pass.
    async fn assess_escape(
        &self,
        document: &Document,
        statements: &[Statement],
    ) -> ReviewResult<(Vec<EscapeProposal>, Usage)>;
    /// Judge one statement, given the statements it depends on.
    async fn judge_statement(
        &self,
        statement: &str,
        context: &str,
    ) -> ReviewResult<(JudgementDraft, Usage)>;
}
