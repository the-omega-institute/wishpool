//! Donated model quota: the encrypted delegated credential, and the hosted
//! worker that spends it on tasks a model can do alone.
//!
//! A donor authorizes the venue in NyxID (incremental consent for delegated
//! access). The venue keeps only the refresh token, encrypted with
//! AES-256-GCM under a deployment key, exchanges it for a short-lived access
//! token when it works, and calls the donor's models through the NyxID LLM
//! gateway. Usage is taken from the gateway's reply and charged to the
//! donor's monthly cap; every contribution records it as metered.

use std::{collections::HashMap, sync::Arc, time::Duration};

use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, AeadCore, OsRng},
};
use async_trait::async_trait;
use base64::Engine as _;
use mongodb::bson::doc;
use tokio::sync::{Mutex, watch};
use wishpool_core::{
    CoreError, CoreResult,
    app::App,
    ids::PersonId,
    model::{
        AgentInfo, Caller, ContributionMode, ContributionOutput, NewContribution, PriorWork,
        TaskKind, TaskStatus, TokenUsage, VerifiedIdentity,
    },
    ports::TaskFilter,
};
use wishpool_review::{ReviewModel, openai_compat::ChatModel, openalex::OpenAlex};

use crate::{
    auth::nyxid::NyxIdClient,
    review::{SearchedPaper, mapping, search_statement},
    store::{MongoStore, unavailable, write_error},
};

/// AES-256-GCM over the delegated refresh token.
#[derive(Clone)]
pub struct TokenCipher {
    cipher: Aes256Gcm,
}

impl TokenCipher {
    /// `key` is 32 bytes, standard base64.
    pub fn from_base64(key: &str) -> anyhow::Result<Self> {
        let bytes = base64::engine::general_purpose::STANDARD.decode(key.trim())?;
        anyhow::ensure!(
            bytes.len() == 32,
            "WISHPOOL_TOKEN_KEY must decode to 32 bytes"
        );
        Ok(Self {
            cipher: Aes256Gcm::new_from_slice(&bytes)?,
        })
    }

    pub fn seal(&self, plaintext: &str) -> CoreResult<String> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|_| CoreError::Unavailable("encryption failed".into()))?;
        let mut sealed = nonce.to_vec();
        sealed.extend(ciphertext);
        Ok(base64::engine::general_purpose::STANDARD.encode(sealed))
    }

    pub fn open(&self, sealed: &str) -> CoreResult<String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(sealed)
            .map_err(|_| CoreError::Unavailable("sealed token is not base64".into()))?;
        if bytes.len() < 13 {
            return Err(CoreError::Unavailable("sealed token too short".into()));
        }
        let (nonce, ciphertext) = bytes.split_at(12);
        let plaintext = self
            .cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| {
                CoreError::Unavailable("sealed token does not open with this key".into())
            })?;
        String::from_utf8(plaintext)
            .map_err(|_| CoreError::Unavailable("sealed token is not UTF-8".into()))
    }
}

/// Sealed refresh tokens by donor.
#[async_trait]
pub trait DonorTokens: Send + Sync {
    async fn put(&self, donor: &PersonId, sealed: &str) -> CoreResult<()>;
    async fn get(&self, donor: &PersonId) -> CoreResult<Option<String>>;
    async fn delete(&self, donor: &PersonId) -> CoreResult<()>;
}

#[derive(Default)]
pub struct MemoryDonorTokens(Mutex<HashMap<PersonId, String>>);

#[async_trait]
impl DonorTokens for MemoryDonorTokens {
    async fn put(&self, donor: &PersonId, sealed: &str) -> CoreResult<()> {
        self.0.lock().await.insert(donor.clone(), sealed.to_owned());
        Ok(())
    }
    async fn get(&self, donor: &PersonId) -> CoreResult<Option<String>> {
        Ok(self.0.lock().await.get(donor).cloned())
    }
    async fn delete(&self, donor: &PersonId) -> CoreResult<()> {
        self.0.lock().await.remove(donor);
        Ok(())
    }
}

const DONOR_TOKENS: &str = "donor_tokens";

#[async_trait]
impl DonorTokens for MongoStore {
    async fn put(&self, donor: &PersonId, sealed: &str) -> CoreResult<()> {
        self.raw(DONOR_TOKENS)
            .replace_one(
                doc! { "_id": donor.as_str() },
                doc! { "_id": donor.as_str(), "sealed": sealed },
            )
            .upsert(true)
            .await
            .map_err(write_error)?;
        Ok(())
    }
    async fn get(&self, donor: &PersonId) -> CoreResult<Option<String>> {
        let found = self
            .raw(DONOR_TOKENS)
            .find_one(doc! { "_id": donor.as_str() })
            .await
            .map_err(unavailable)?;
        Ok(found.and_then(|d| d.get_str("sealed").ok().map(str::to_owned)))
    }
    async fn delete(&self, donor: &PersonId) -> CoreResult<()> {
        self.raw(DONOR_TOKENS)
            .delete_one(doc! { "_id": donor.as_str() })
            .await
            .map_err(unavailable)?;
        Ok(())
    }
}

/// Everything the donation flow and the hosted worker share.
pub struct Donations {
    pub tokens: Arc<dyn DonorTokens>,
    pub cipher: TokenCipher,
    pub scope: String,
    pub service_ids: Vec<String>,
    pub gateway_url: String,
    pub default_model: String,
}

/// Models a donor may choose; the gateway routes them by name prefix.
pub fn valid_model(model: &str) -> bool {
    (3..=100).contains(&model.len())
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '/'))
}

/// Tokens reserved before a call; a call is skipped when less remains.
const CALL_RESERVE: u64 = 40_000;

pub struct HostedWorker {
    pub app: Arc<App>,
    pub nyxid: Arc<NyxIdClient>,
    pub donations: Arc<Donations>,
    pub openalex: Arc<OpenAlex>,
    pub interval: Duration,
}

enum Outcome {
    Submitted,
    Idle,
}

impl HostedWorker {
    pub async fn run(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) {
        loop {
            match self.round().await {
                Ok(0) => {}
                Ok(n) => tracing::info!(contributions = n, "hosted round"),
                Err(error) => tracing::warn!(%error, "hosted round failed"),
            }
            tokio::select! {
                _ = tokio::time::sleep(self.interval) => {}
                _ = shutdown.changed() => break,
            }
        }
    }

    #[cfg(test)]
    pub async fn round_for_test(&self) -> CoreResult<usize> {
        self.round().await
    }

    /// One task per donor with budget left.
    async fn round(&self) -> CoreResult<usize> {
        let mut done = 0;
        for grant in self.app.spendable_donations().await? {
            if grant.remaining(chrono::Utc::now()) < CALL_RESERVE {
                continue;
            }
            match self.work_for(&grant.donor, &grant.model).await {
                Ok(Outcome::Submitted) => done += 1,
                Ok(Outcome::Idle) => {}
                Err(CoreError::Unauthenticated) => {
                    tracing::warn!(donor = %grant.donor, "delegated credential refused; revoking the grant");
                    self.app.revoke_donation(&grant.donor).await?;
                    self.donations.tokens.delete(&grant.donor).await?;
                }
                Err(error) => tracing::warn!(donor = %grant.donor, %error, "hosted task failed"),
            }
        }
        Ok(done)
    }

    async fn access_token(&self, donor: &PersonId) -> CoreResult<String> {
        let sealed = self
            .donations
            .tokens
            .get(donor)
            .await?
            .ok_or(CoreError::Unauthenticated)?;
        let refresh = self.donations.cipher.open(&sealed)?;
        let tokens = self.nyxid.refresh(&refresh).await?;
        if let Some(rotated) = &tokens.refresh_token {
            self.donations
                .tokens
                .put(donor, &self.donations.cipher.seal(rotated)?)
                .await?;
        }
        Ok(tokens.access_token)
    }

    async fn pick(&self, donor: &Caller) -> CoreResult<Option<wishpool_core::model::Task>> {
        for kind in [TaskKind::JudgeEscape, TaskKind::LiteratureCheck] {
            let page = self
                .app
                .list_tasks(
                    TaskFilter {
                        kind: Some(kind),
                        status: Some("open".into()),
                        ..Default::default()
                    },
                    Some(50),
                    None,
                )
                .await?;
            if let Some(task) = page
                .items
                .into_iter()
                .find(|t| !t.contributors.contains(&donor.person))
            {
                return Ok(Some(task));
            }
        }
        Ok(None)
    }

    async fn work_for(&self, donor: &PersonId, model: &str) -> CoreResult<Outcome> {
        let caller = self
            .app
            .caller(&VerifiedIdentity {
                subject: donor.clone(),
                name: None,
                email: None,
                picture: None,
            })
            .await?;
        let Some(task) = self.pick(&caller).await? else {
            return Ok(Outcome::Idle);
        };
        let task = match self
            .app
            .lease_task(&caller, &task.id, ContributionMode::Hosted)
            .await
        {
            Ok(task) => task,
            Err(CoreError::Conflict(_)) => return Ok(Outcome::Idle),
            Err(error) => return Err(error),
        };
        let result = self.perform(&caller, &task, model).await;
        if result.is_err()
            && matches!(
                self.app.task(&task.id).await.map(|t| t.status),
                Ok(TaskStatus::Leased { .. })
            )
        {
            let _ = self.app.release_task(&caller, &task.id).await;
        }
        result
    }

    async fn perform(
        &self,
        caller: &Caller,
        task: &wishpool_core::model::Task,
        model: &str,
    ) -> CoreResult<Outcome> {
        let token = self.access_token(&caller.person).await?;
        let chat = ChatModel::new(&self.donations.gateway_url, token, model.to_owned())
            .map_err(|e| CoreError::Unavailable(e.to_string()))?;
        let review = |e: wishpool_review::ReviewError| match e {
            wishpool_review::ReviewError::Provider {
                status: 401 | 403, ..
            } => CoreError::Unauthenticated,
            other => CoreError::Unavailable(other.to_string()),
        };
        let context = self.app.task_context(caller, &task.id).await?;
        let (output, usage) = match task.kind {
            TaskKind::JudgeEscape => {
                let mut background = format!(
                    "PAPER: {}\nABSTRACT: {}\n",
                    context.paper_title, context.abstract_text
                );
                for dependency in &context.dependencies {
                    background.push_str(&format!(
                        "\n{}: {}\n",
                        dependency.label, dependency.statement
                    ));
                }
                let (draft, usage) = chat
                    .judge_statement(&context.claim.statement, &background)
                    .await
                    .map_err(review)?;
                let output = mapping::draft_judgement(&draft).ok_or_else(|| {
                    CoreError::Unavailable(format!(
                        "model returned an unusable judgement ({:?})",
                        draft.shape
                    ))
                })?;
                (output, usage)
            }
            TaskKind::LiteratureCheck => {
                let paper = SearchedPaper {
                    title: &context.paper_title,
                    abstract_text: &context.abstract_text,
                    doi: None,
                };
                let (found, usage) = search_statement(
                    &chat,
                    &self.openalex,
                    &paper,
                    &context.claim.id,
                    &context.claim.statement,
                )
                .await
                .map_err(review)?;
                let draft = mapping::literature(std::slice::from_ref(&found), model);
                let prior: Vec<PriorWork> = match draft.payload {
                    wishpool_core::model::StagePayload::Literature { prior, .. } => prior,
                    _ => vec![],
                };
                let summary = format!(
                    "{} of {} OpenAlex candidates bear on {} (model {model}); an editor checks each.",
                    prior.len(),
                    found.candidates.len(),
                    context.claim.label
                );
                (
                    ContributionOutput::Literature {
                        prior,
                        searched: found
                            .queries
                            .iter()
                            .map(|q| format!("OpenAlex works search: {q}"))
                            .collect(),
                        summary,
                    },
                    usage,
                )
            }
            TaskKind::Formalize | TaskKind::Probe => return Ok(Outcome::Idle),
        };
        let tokens = TokenUsage {
            input: usage.input,
            output: usage.output,
            metered: true,
        };
        let contribution = NewContribution {
            agent: AgentInfo {
                tool: "wishpool-hosted".into(),
                model: model.to_owned(),
            },
            output,
            tokens: Some(tokens),
        };
        self.app
            .submit_contribution_as(caller, &task.id, contribution)
            .await?;
        self.app
            .charge_donation(&caller.person, tokens.total())
            .await?;
        Ok(Outcome::Submitted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_tokens_round_trip_and_resist_other_keys() {
        let key = base64::engine::general_purpose::STANDARD.encode([7_u8; 32]);
        let other = base64::engine::general_purpose::STANDARD.encode([8_u8; 32]);
        let cipher = TokenCipher::from_base64(&key).unwrap();
        let sealed = cipher.seal("refresh-token").unwrap();
        assert_ne!(
            sealed,
            cipher.seal("refresh-token").unwrap(),
            "fresh nonce every time"
        );
        assert_eq!(cipher.open(&sealed).unwrap(), "refresh-token");
        assert!(
            TokenCipher::from_base64(&other)
                .unwrap()
                .open(&sealed)
                .is_err()
        );
        assert!(TokenCipher::from_base64("c2hvcnQ=").is_err());
    }

    #[test]
    fn model_names() {
        assert!(valid_model("claude-opus-5-5"));
        assert!(!valid_model("x"));
        assert!(!valid_model("rm -rf"));
    }
}
