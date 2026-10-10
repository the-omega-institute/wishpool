//! A model-stated opened source is a report, never a venue verification.
use wishpool_core::model::{Source, SourceKind};

/// Keep the title and the exact locator together; never manufacture an identifier.
pub(super) fn reported_work(work: &str) -> bool {
    let Some((title, tail)) = work.rsplit_once(" [opened: ") else {
        return false;
    };
    let Some(locator) = tail.strip_suffix(']') else {
        return false;
    };
    if title.trim().is_empty() || work.chars().count() > 2_000 {
        return false;
    }
    let (kind, locator) = if let Some(doi) = locator.strip_prefix("DOI:") {
        (SourceKind::Doi, doi)
    } else if let Some(arxiv) = locator.strip_prefix("arXiv:") {
        (SourceKind::Arxiv, arxiv)
    } else {
        (SourceKind::Url, locator)
    };
    Source {
        kind,
        locator: locator.into(),
        year: None,
    }
    .validate()
    .is_ok()
}
