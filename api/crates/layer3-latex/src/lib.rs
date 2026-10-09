//! Layer 3: LaTeX papers.
//!
//! [`archive`] unpacks what an author uploads (one `.tex` file, a `.zip`, or a
//! `.tar.gz`) with limits on size, count and paths. [`parse`] finds the main
//! file, inlines `\input`/`\include`, and reads the title, authors, abstract
//! and every statement environment declared with `\newtheorem` (or a common
//! default such as `theorem`, `lemma`, `conjecture`). [`compile`] renders a
//! PDF with TeX Live as arXiv does, without shell escape.
//!
//! The vocabulary is LaTeX's own: environments, labels, optional titles.

pub mod archive;
pub mod compile;
pub mod parse;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LatexError {
    #[error("archive: {0}")]
    Archive(String),
    #[error("no main file: {0}")]
    NoMainFile(String),
    #[error("compile: {0}")]
    Compile(String),
}

pub type LatexResult<T> = Result<T, LatexError>;

/// What kind of statement an environment holds, by its printed name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatementKind {
    Theorem,
    Proposition,
    Lemma,
    Corollary,
    Claim,
    Conjecture,
    /// Open questions and problems posed in the paper.
    Question,
}

impl StatementKind {
    /// Classify an environment by its printed name ("Theorem", "Main
    /// Theorem", "Conjecture", "Hauptsatz" is not recognised). Definitions,
    /// remarks and examples return `None`.
    pub fn from_name(name: &str) -> Option<Self> {
        let n = name.to_lowercase();
        let has = |w: &str| n.split(|c: char| !c.is_alphanumeric()).any(|t| t == w);
        Some(if has("conjecture") || has("hypothesis") {
            Self::Conjecture
        } else if has("question") || has("problem") {
            Self::Question
        } else if has("theorem") || has("thm") {
            Self::Theorem
        } else if has("proposition") || has("prop") {
            Self::Proposition
        } else if has("lemma") || has("lem") {
            Self::Lemma
        } else if has("corollary") || has("cor") {
            Self::Corollary
        } else if has("claim") {
            Self::Claim
        } else {
            return None;
        })
    }

    pub fn is_open(self) -> bool {
        matches!(self, Self::Conjecture | Self::Question)
    }
}

/// One statement environment as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    /// 1-based position among the statements of the paper.
    pub index: usize,
    pub kind: StatementKind,
    /// The environment name in the source, e.g. `thm`.
    pub environment: String,
    /// The printed name, e.g. `Theorem`.
    pub display_name: String,
    /// The optional argument, e.g. `\begin{theorem}[Main estimate]`.
    pub title: Option<String>,
    pub label: Option<String>,
    /// The body as LaTeX, with the label removed.
    pub body: String,
    /// A `proof` environment follows directly.
    pub has_proof: bool,
    /// The section the statement appears in.
    pub section: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedPaper {
    pub main_file: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub abstract_text: Option<String>,
    pub statements: Vec<Statement>,
    /// Math macros defined in the preamble, name → body (see
    /// [`parse::macros`]).
    pub macros: std::collections::BTreeMap<String, String>,
    /// Inputs that could not be resolved, and similar notes.
    pub warnings: Vec<String>,
}
