use serde::{Deserialize, Serialize};

use crate::{CoreError, CoreResult};

/// Where a statement, problem or paper is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Doi,
    Arxiv,
    Hexagon,
    Zenodo,
    Oeis,
    Url,
    /// Named work from a paper/report, without inventing a public identifier.
    NamedWork,
    /// Communicated directly by the poser; no public locator.
    Personal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub kind: SourceKind,
    pub locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
}

impl Source {
    pub fn validate(&self) -> CoreResult<()> {
        let locator = self.locator.trim();
        if self.kind != SourceKind::Personal && locator.is_empty() {
            return Err(CoreError::invalid("source locator is required"));
        }
        let ok = match self.kind {
            SourceKind::Doi => locator.starts_with("10.") && locator.contains('/'),
            SourceKind::Arxiv => is_arxiv_id(locator),
            SourceKind::Oeis => {
                locator.len() == 7
                    && locator.starts_with('A')
                    && locator[1..].chars().all(|c| c.is_ascii_digit())
            }
            SourceKind::Zenodo => locator.starts_with("10.5281/zenodo.") || is_web_url(locator),
            SourceKind::Hexagon | SourceKind::Url => is_web_url(locator),
            SourceKind::Personal | SourceKind::NamedWork => true,
        };
        if !ok {
            return Err(CoreError::invalid(format!(
                "locator {locator:?} is not a valid {:?} identifier",
                self.kind
            )));
        }
        if let Some(year) = self.year
            && !(1600..=2100).contains(&year)
        {
            return Err(CoreError::invalid("source year out of range"));
        }
        Ok(())
    }
}

/// A DOI in its bare form: it starts with `10.`, contains a slash followed by
/// a non-empty suffix, contains no whitespace, and is at most 256 characters.
pub fn is_doi(value: &str) -> bool {
    value.chars().count() <= 256
        && value.starts_with("10.")
        && !value.chars().any(char::is_whitespace)
        && value
            .split_once('/')
            .is_some_and(|(_, suffix)| !suffix.is_empty())
}

/// Remove the DOI resolver prefixes accepted at paper-upload boundaries.
pub fn normalise_doi(value: &str) -> String {
    let value = value.trim();
    for prefix in [
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
        "doi:",
    ] {
        if value
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            return value[prefix.len()..].trim().to_owned();
        }
    }
    value.to_owned()
}

/// New-style arXiv identifier `YYMM.NNNNN` with an optional `vN` suffix, or an
/// old-style `archive/NNNNNNN`.
pub fn is_arxiv_id(value: &str) -> bool {
    let core = match value.rsplit_once('v') {
        Some((head, tail)) if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) => head,
        _ => value,
    };
    if let Some((yymm, number)) = core.split_once('.') {
        return yymm.len() == 4
            && yymm.chars().all(|c| c.is_ascii_digit())
            && (4..=5).contains(&number.len())
            && number.chars().all(|c| c.is_ascii_digit());
    }
    if let Some((archive, number)) = core.split_once('/') {
        return !archive.is_empty()
            && archive
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-' || c == '.')
            && number.len() == 7
            && number.chars().all(|c| c.is_ascii_digit());
    }
    false
}

pub fn is_web_url(value: &str) -> bool {
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"));
    matches!(rest, Some(host) if host.split('/').next().is_some_and(|h| h.contains('.')))
}

pub(crate) fn require_text(field: &str, value: &str, max: usize) -> CoreResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CoreError::invalid(format!("{field} is required")));
    }
    if trimmed.chars().count() > max {
        return Err(CoreError::invalid(format!(
            "{field} exceeds {max} characters"
        )));
    }
    Ok(())
}

/// Mathematics Subject Classification code, e.g. `11B39` or `05C`.
pub(crate) fn validate_msc(codes: &[String]) -> CoreResult<()> {
    if codes.len() > 8 {
        return Err(CoreError::invalid("at most 8 MSC codes"));
    }
    for code in codes {
        let ok = (2..=5).contains(&code.len())
            && code[..2].chars().all(|c| c.is_ascii_digit())
            && code[2..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-');
        if !ok {
            return Err(CoreError::invalid(format!("{code:?} is not an MSC code")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arxiv_ids() {
        assert!(is_arxiv_id("2609.33421"));
        assert!(is_arxiv_id("2609.33421v2"));
        assert!(is_arxiv_id("math/0601001"));
        assert!(!is_arxiv_id("2609.3"));
        assert!(!is_arxiv_id("hello"));
    }

    #[test]
    fn dois() {
        assert!(is_doi("10.48550/arXiv.2609.33421"));
        assert!(is_doi("10.1000/xyz"));
        assert!(!is_doi("10.1000/"));
        assert!(!is_doi("10.1000/has whitespace"));
        assert!(!is_doi("10.1000"));
        assert!(!is_doi("10.1000/has\u{0085}whitespace"));
        assert!(is_doi(&format!("10.1000/{}", "x".repeat(248))));
        assert!(!is_doi(&format!("10.1000/{}", "x".repeat(249))));
        assert!(is_doi(&format!("10.1000/{}", "𝑥".repeat(248))));
        assert_eq!(normalise_doi("  DOI: 10.1000/xyz  "), "10.1000/xyz");
        assert_eq!(normalise_doi("https://doi.org/10.1000/xyz"), "10.1000/xyz");
        assert_eq!(
            normalise_doi("http://dx.doi.org/10.1000/xyz"),
            "10.1000/xyz"
        );
    }

    #[test]
    fn msc_codes() {
        assert!(validate_msc(&["11B39".into(), "05C".into()]).is_ok());
        assert!(validate_msc(&["B39".into()]).is_err());
    }
}
