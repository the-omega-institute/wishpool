//! A paper submitted by its author: the LaTeX source, the statements read
//! from it, the analysis, and what follows acceptance.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{
    claim::{Claim, ClaimKind, ClaimRole},
    common::{is_doi, normalise_doi, require_text, validate_msc},
    formalization::FormalizationPlan,
    review::{Stage, StagePayload, StageReport},
};
use crate::{
    CoreError, CoreResult,
    ids::{ClaimId, PersonId, RecordId, SubmissionId},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<PersonId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orcid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affiliation: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiUse {
    None,
    Assisted,
    Substantial,
    Primarily,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiDisclosure {
    pub level: AiUse,
    pub statement: String,
}

/// A stored file: the uploaded source archive or a compiled PDF.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRef {
    pub id: String,
    pub bytes: u64,
    pub sha256: String,
}

/// One uploaded version of the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperVersion {
    pub number: u32,
    pub archive: BlobRef,
    pub filename: String,
    pub main_file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf: Option<BlobRef>,
    /// The compiler's message when the PDF could not be built.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compile_error: Option<String>,
    #[serde(default)]
    pub parse_warnings: Vec<String>,
    /// Math macros the preamble defines, for rendering statements.
    #[serde(default)]
    pub macros: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub note: String,
    pub uploaded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SubmissionStatus {
    /// Statements read from the source await the author's confirmation.
    Draft,
    InReview,
    Accepted {
        record: RecordId,
    },
    /// Below the threshold; the report stays private to the author, who may
    /// upload a revised version.
    NotAccepted,
    Withdrawn,
}

impl SubmissionStatus {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::InReview => "in_review",
            Self::Accepted { .. } => "accepted",
            Self::NotAccepted => "not_accepted",
            Self::Withdrawn => "withdrawn",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Draft | Self::InReview)
    }
}

/// All submissions use the same sequence and review pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionKind {
    #[default]
    Paper,
    Note,
    Conjecture,
}

/// Typed conjectures become ordinary LaTeX sources before reading and storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypedConjecture {
    pub title: String,
    pub statement: String,
    #[serde(default)]
    pub background: String,
    #[serde(default)]
    pub origin: String,
}

impl TypedConjecture {
    pub fn source(&self, authors: &[Author]) -> CoreResult<Vec<u8>> {
        require_text("title", &self.title, 1_000)?;
        require_text("statement", &self.statement, 20_000)?;
        if self.background.chars().count() > 50_000 || self.origin.chars().count() > 2_000 {
            return Err(CoreError::invalid(
                "background or origin exceeds its length limit",
            ));
        }
        // The statement/background are untrusted TeX, just like uploaded source.
        // Keep the generated wrapper intact so exactly one conjecture is read.
        for text in [&self.statement, &self.background] {
            if text.contains("\\begin{conjecture}")
                || text.contains("\\end{conjecture}")
                || text.contains("\\begin{document}")
                || text.contains("\\end{document}")
            {
                return Err(CoreError::invalid(
                    "type the mathematical text without document or conjecture environments",
                ));
            }
        }
        fn plain(text: &str) -> String {
            text.chars()
                .map(|c| match c {
                    '\\' => "\\textbackslash{}".into(),
                    '{' | '}' | '%' | '&' | '#' | '_' | '$' => format!("\\{c}"),
                    '^' => "\\textasciicircum{}".into(),
                    '~' => "\\textasciitilde{}".into(),
                    _ => c.to_string(),
                })
                .collect()
        }
        Ok(format!(
            r"\documentclass{{article}}
\usepackage{{amsmath,amssymb,amsthm}}
\newtheorem{{conjecture}}{{Conjecture}}
\title{{{}}}
\author{{{}}}
\begin{{document}}
\maketitle
\begin{{abstract}}
{}
\end{{abstract}}
\begin{{conjecture}}
{}
\end{{conjecture}}
\paragraph{{Origin}} {}
\end{{document}}
",
            plain(&self.title),
            authors
                .iter()
                .map(|a| plain(&a.name))
                .collect::<Vec<_>>()
                .join(r" \and "),
            self.background,
            self.statement,
            plain(&self.origin)
        )
        .into_bytes())
    }
}

fn public_by_default() -> bool {
    true
}

/// Public mathematical details after acceptance, selected at upload and
/// changeable later. Legacy documents retain their undecided state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Undecided,
    Public,
    Private,
}

/// What became of a conjecture or question the paper poses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ConjectureState {
    /// Being checked: still open, within reach.
    Screening,
    /// Someone is working on it.
    TakenUp,
    NotPursued {
        reason: String,
    },
    Settled {
        outcome: ConjectureOutcome,
        summary: String,
        evidence: Vec<super::review::Evidence>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConjectureOutcome {
    Proved,
    Disproved,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConjectureFollowUp {
    pub claim: ClaimId,
    pub state: ConjectureState,
    pub updated_at: DateTime<Utc>,
}

/// Follow-up state for the immutable publication, retained when its author revises.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedProgress {
    pub version: u32,
    pub claims_revision: u64,
    pub formalization: FormalizationPlan,
    pub conjectures: Vec<ConjectureFollowUp>,
    /// Legacy records lack pinned inputs; capture them before the first revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<Box<Submission>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submission {
    /// Explicitly requested at new acceptance or by staff; legacy documents do
    /// not opt into a backfill merely because reconciliation visits them.
    #[serde(default)]
    pub problem_check_requested: bool,
    #[serde(default)]
    pub conjecture_dependencies: Vec<super::ConfirmedDependency>,
    #[serde(default)]
    pub kind: SubmissionKind,
    #[serde(default)]
    pub lean_statements: Vec<super::LeanStatementAttempt>,
    pub id: SubmissionId,
    pub submitter: PersonId,
    pub title: String,
    pub abstract_text: String,
    pub authors: Vec<Author>,
    pub ai_disclosure: AiDisclosure,
    #[serde(default)]
    pub msc: Vec<String>,
    /// The paper's persistent identifier, when it already has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    /// Versions in upload order; the last is current.
    pub versions: Vec<PaperVersion>,
    /// Statements read from the current version, awaiting confirmation.
    #[serde(default)]
    pub extracted: Vec<Claim>,
    /// The confirmed statements the analysis works on.
    #[serde(default)]
    pub claims: Vec<Claim>,
    pub claims_revision: u64,
    #[serde(default)]
    pub reports: Vec<StageReport>,
    pub status: SubmissionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<crate::policy::Decision>,
    /// The author lets volunteer contributors read the statements.
    #[serde(default)]
    pub open_to_contributors: bool,
    #[serde(default)]
    pub analysis_visibility: Visibility,
    #[serde(default)]
    pub formalization: FormalizationPlan,
    #[serde(default)]
    pub conjectures: Vec<ConjectureFollowUp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_progress: Option<PublishedProgress>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
}

/// Metadata the author supplies with an upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewPaper {
    #[serde(default)]
    pub kind: SubmissionKind,
    #[serde(default = "public_by_default")]
    pub make_public_after_acceptance: bool,
    #[serde(default)]
    pub typed_conjecture: Option<TypedConjecture>,
    pub ai_disclosure: AiDisclosure,
    /// Overrides the authors read from `\author`.
    #[serde(default)]
    pub authors: Vec<Author>,
    #[serde(default)]
    pub msc: Vec<String>,
    #[serde(default)]
    pub doi: Option<String>,
    #[serde(default)]
    pub open_to_contributors: bool,
}

impl NewPaper {
    pub fn validate(&self) -> CoreResult<()> {
        if self.typed_conjecture.is_some() && self.kind != SubmissionKind::Conjecture {
            return Err(CoreError::invalid("typed input is only for conjectures"));
        }
        require_text(
            "AI disclosure statement",
            &self.ai_disclosure.statement,
            5_000,
        )?;
        for author in &self.authors {
            require_text("author name", &author.name, 200)?;
        }
        if self.authors.len() > 50 {
            return Err(CoreError::invalid("at most 50 authors"));
        }
        if let Some(value) = &self.doi {
            let id = normalise_doi(value);
            if !id.is_empty() && !is_doi(&id) {
                return Err(CoreError::invalid(format!("{id:?} is not a DOI")));
            }
        }
        validate_msc(&self.msc)
    }

    /// Normalize the optional DOI at the Layer 2 input boundary.
    pub fn normalise_doi(&mut self) {
        self.doi = self
            .doi
            .as_deref()
            .map(normalise_doi)
            .filter(|doi| !doi.is_empty());
    }
}

/// The author's confirmation of one extracted statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimConfirmation {
    #[serde(default)]
    pub depends_on_conjectures: Vec<super::ConjectureRef>,
    pub id: ClaimId,
    pub kind: ClaimKind,
    pub role: ClaimRole,
    #[serde(default)]
    pub depends_on: Vec<ClaimId>,
    #[serde(default)]
    pub settles: Option<super::claim::OpenProblemRef>,
    /// Exclude a statement wrongly read from the source.
    #[serde(default)]
    pub excluded: bool,
}

impl Submission {
    pub fn current_version(&self) -> Option<&PaperVersion> {
        self.versions.last()
    }

    pub fn involves(&self, person: &PersonId) -> bool {
        &self.submitter == person
            || self
                .authors
                .iter()
                .any(|a| a.person.as_ref() == Some(person))
    }

    pub fn is_open(&self) -> bool {
        self.status == SubmissionStatus::InReview
    }

    /// The authoritative report for a stage: the latest valid for the
    /// current claim set.
    pub fn latest_report(&self, stage: Stage) -> Option<&StageReport> {
        self.reports.iter().rev().find(|r| {
            r.stage == stage
                && (!stage.depends_on_claims() || r.claims_revision == self.claims_revision)
        })
    }

    /// Append a report. A claims report replaces the claim set and voids
    /// claim-dependent reports.
    pub fn push_report(&mut self, report: StageReport) {
        if let StagePayload::Claims { claims } = &report.payload
            && &self.claims != claims
        {
            self.claims = claims.clone();
            self.claims_revision += 1;
        }
        let mut report = report;
        report.claims_revision = self.claims_revision;
        self.updated_at = report.filed_at;
        self.reports.push(report);
    }

    pub fn claim(&self, id: &ClaimId) -> Option<&Claim> {
        self.claims.iter().find(|c| &c.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_paper(doi: Option<&str>) -> NewPaper {
        NewPaper {
            kind: crate::model::SubmissionKind::Paper,
            make_public_after_acceptance: true,
            typed_conjecture: None,
            ai_disclosure: AiDisclosure {
                level: AiUse::None,
                statement: "No AI was used.".into(),
            },
            authors: vec![],
            msc: vec![],
            doi: doi.map(str::to_owned),
            open_to_contributors: false,
        }
    }

    #[test]
    fn normalises_optional_doi_and_validates_bare_identifier() {
        for value in [
            "10.1000/AbC",
            " https://doi.org/10.1000/AbC ",
            "http://dx.doi.org/10.1000/AbC",
            " DOI: 10.1000/AbC ",
        ] {
            let mut paper = new_paper(Some(value));
            assert!(paper.validate().is_ok());
            paper.normalise_doi();
            assert_eq!(paper.doi.as_deref(), Some("10.1000/AbC"));
            assert!(paper.validate().is_ok());
        }
        for value in [None, Some(""), Some("   "), Some("doi: ")] {
            let mut paper = new_paper(value);
            paper.normalise_doi();
            assert_eq!(paper.doi, None);
            assert!(paper.validate().is_ok());
        }
        assert_eq!(
            new_paper(Some("2609.33421")).validate(),
            Err(CoreError::Invalid("\"2609.33421\" is not a DOI".into()))
        );
        for value in ["10.1000/", "10.1000/has whitespace", "10.1000"] {
            assert!(new_paper(Some(value)).validate().is_err());
        }
    }

    #[test]
    fn serialises_doi_without_an_arxiv_alias() {
        let paper = new_paper(Some("10.1000/xyz"));
        let mut json = serde_json::to_value(&paper).unwrap();
        assert_eq!(json["doi"], "10.1000/xyz");
        assert!(json.get("arxiv").is_none());
        json.as_object_mut().unwrap().remove("doi");
        json["arxiv"] = "2609.33421".into();
        let paper: NewPaper = serde_json::from_value(json).unwrap();
        assert_eq!(paper.doi, None);
    }
    #[test]
    fn upload_checkbox_defaults_public_and_all_kinds_round_trip() {
        let mut value = serde_json::to_value(new_paper(None)).unwrap();
        value.as_object_mut().unwrap().remove("kind");
        value
            .as_object_mut()
            .unwrap()
            .remove("make_public_after_acceptance");
        let legacy: NewPaper = serde_json::from_value(value).unwrap();
        assert_eq!(legacy.kind, SubmissionKind::Paper);
        assert!(legacy.make_public_after_acceptance);
        for kind in [
            SubmissionKind::Paper,
            SubmissionKind::Note,
            SubmissionKind::Conjecture,
        ] {
            let mut new = legacy.clone();
            new.kind = kind;
            assert_eq!(
                serde_json::from_value::<NewPaper>(serde_json::to_value(&new).unwrap()).unwrap(),
                new
            );
        }
    }
    #[test]
    fn typed_input_has_limits_and_keeps_the_wrapper_intact() {
        let mut typed = TypedConjecture {
            title: "A title".into(),
            statement: "$x = y$".into(),
            background: String::new(),
            origin: "my own".into(),
        };
        assert!(typed.source(&[]).is_ok());
        typed.statement = "x".repeat(20_001);
        assert!(typed.source(&[]).is_err());
        typed.statement = "$x = y$".into();
        typed.background = "x".repeat(50_001);
        assert!(typed.source(&[]).is_err());
        typed.background.clear();
        typed.statement = r"\end{conjecture}".into();
        assert!(typed.source(&[]).is_err());
    }
}
