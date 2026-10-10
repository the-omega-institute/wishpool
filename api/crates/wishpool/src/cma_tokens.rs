//! Reuse sealed delegated-token refresh; fall back to an operator-mounted user token.
use crate::donations::HostedWorker;
use async_trait::async_trait;
use std::{path::PathBuf, sync::Arc};
use wishpool_core::ids::PersonId;
use wishpool_review::{ReviewError, ReviewResult, cma::TokenSource};
pub struct CmaTokens {
    pub hosted: Option<Arc<HostedWorker>>,
    pub account: Option<PersonId>,
    pub file: Option<PathBuf>,
}
#[async_trait]
impl TokenSource for CmaTokens {
    async fn token(&self) -> ReviewResult<String> {
        if let (Some(hosted), Some(account)) = (&self.hosted, &self.account) {
            match hosted.access_token(account).await {
                Ok(token) => return Ok(token),
                Err(_) if self.file.is_some() => {
                    tracing::warn!("CMA delegated token unavailable; using configured token file")
                }
                Err(_) => {
                    return Err(ReviewError::Transport(
                        "CMA delegated token unavailable".into(),
                    ));
                }
            }
        }
        let file = self
            .file
            .as_ref()
            .ok_or_else(|| ReviewError::Output("CMA user token is not configured".into()))?;
        if std::fs::metadata(file)
            .map_err(|_| ReviewError::Output("CMA token file unavailable".into()))?
            .len()
            > 64 * 1024
        {
            return Err(ReviewError::Output("CMA token file too large".into()));
        }
        std::fs::read_to_string(file)
            .map_err(|_| ReviewError::Output("CMA token file unavailable".into()))
    }
}
