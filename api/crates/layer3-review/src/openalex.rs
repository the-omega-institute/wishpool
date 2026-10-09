//! OpenAlex (https://api.openalex.org): CC0 scholarly metadata, used to
//! ground literature checks in works that exist. Usage is metered by
//! OpenAlex per request; pass an API key in production.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{ReviewError, ReviewResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Work {
    /// OpenAlex id, e.g. `https://openalex.org/W2741809807`.
    pub id: String,
    pub title: String,
    pub doi: Option<String>,
    pub year: Option<i32>,
    pub abstract_text: String,
}

/// Works whose primary topic lies in Mathematics, Computer Science or
/// Physics and Astronomy. Unfiltered keyword search returns unrelated
/// fields that share a word (e.g. "recurrence" in oncology).
pub const FIELD_FILTER: &str = "primary_topic.field.id:fields/26|fields/17|fields/31";

pub struct OpenAlex {
    http: reqwest::Client,
    base: String,
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct Page {
    results: Vec<RawWork>,
}

#[derive(Deserialize)]
struct RawWork {
    id: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    doi: Option<String>,
    #[serde(default)]
    publication_year: Option<i32>,
    #[serde(default)]
    abstract_inverted_index: Option<std::collections::BTreeMap<String, Vec<usize>>>,
}

/// Rebuild an abstract from OpenAlex's inverted index.
fn abstract_text(index: Option<std::collections::BTreeMap<String, Vec<usize>>>) -> String {
    let Some(index) = index else {
        return String::new();
    };
    let mut positions: Vec<(usize, String)> = index
        .into_iter()
        .flat_map(|(w, ps)| ps.into_iter().map(move |p| (p, w.clone())))
        .collect();
    positions.sort();
    positions
        .into_iter()
        .map(|(_, w)| w)
        .collect::<Vec<_>>()
        .join(" ")
}

impl OpenAlex {
    pub fn new(base: &str, api_key: Option<String>) -> ReviewResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("wishpool-review/0.1 (+https://github.com/the-omega-institute)")
            .build()
            .map_err(|e| ReviewError::Transport(e.to_string()))?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            api_key,
        })
    }

    /// Search works by free text, best matches first.
    pub async fn search(&self, query: &str, limit: usize) -> ReviewResult<Vec<Work>> {
        let query: String = query
            .chars()
            .filter(|c| !matches!(c, '$' | '\\' | '{' | '}'))
            .take(300)
            .collect();
        let mut request = self.http.get(format!("{}/works", self.base)).query(&[
            ("search", query.as_str()),
            ("filter", FIELD_FILTER),
            ("per-page", &limit.clamp(1, 25).to_string()),
            (
                "select",
                "id,display_name,doi,publication_year,abstract_inverted_index",
            ),
        ]);
        if let Some(key) = &self.api_key {
            request = request.query(&[("api_key", key.as_str())]);
        }
        let response = request
            .send()
            .await
            .map_err(|e| ReviewError::Transport(e.to_string()))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return Err(ReviewError::Provider {
                status,
                body: body.chars().take(500).collect(),
            });
        }
        let page: Page = response
            .json()
            .await
            .map_err(|e| ReviewError::Output(e.to_string()))?;
        Ok(page
            .results
            .into_iter()
            .map(|w| Work {
                id: w.id,
                title: w.display_name.unwrap_or_default(),
                doi: w
                    .doi
                    .map(|d| d.trim_start_matches("https://doi.org/").to_owned()),
                year: w.publication_year,
                abstract_text: abstract_text(w.abstract_inverted_index),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use axum::{Json, Router, extract::Query, routing::get};
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn searches_and_rebuilds_abstracts() {
        let router = Router::new().route(
            "/works",
            get(
                |Query(q): Query<std::collections::HashMap<String, String>>| async move {
                    assert!(!q["search"].contains('$'));
                    Json(json!({ "results": [{
                    "id": "https://openalex.org/W1",
                    "display_name": "Sumfree sets",
                    "doi": "https://doi.org/10.1000/x",
                    "publication_year": 2020,
                    "abstract_inverted_index": { "sets": [1], "Sumfree": [0], "periodic": [2] }
                }] }))
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let works = OpenAlex::new(&base, None)
            .unwrap()
            .search("sum-free $x$ periodic", 5)
            .await
            .unwrap();
        assert_eq!(works[0].doi.as_deref(), Some("10.1000/x"));
        assert_eq!(works[0].abstract_text, "Sumfree sets periodic");
    }
}
