//! Tasks, contributions, judgements and donation grants.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use wishpool_core::{
    CoreError, CoreResult,
    ids::{PersonId, SubmissionId},
    judgement::ClaimJudgement,
    model::{Contribution, Task, TaskKind},
    ports::*,
};

use super::*;

pub(crate) const TASKS: &str = "tasks";
pub(crate) const CONTRIBUTIONS: &str = "contributions";
pub(crate) const JUDGEMENTS: &str = "judgements";
pub(crate) const GRANTS: &str = "donation_grants";

fn kind_value(kind: TaskKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Task documents carry the lease expiry as a BSON date beside the domain
/// form, so expiry queries can use an index.
fn task_document(task: &Task) -> CoreResult<Document> {
    let mut document = to_document(task)?;
    let (holder, until) = match &task.status {
        wishpool_core::model::TaskStatus::Leased { lease } => (
            Some(lease.holder.0.clone()),
            Some(BsonDateTime::from_chrono(lease.until)),
        ),
        _ => (None, None),
    };
    document.insert("lease_holder", holder);
    document.insert("lease_until", until);
    document.insert("target_key", task.target.key());
    Ok(document)
}

pub(crate) async fn ensure_indexes(store: &MongoStore) -> anyhow::Result<()> {
    use mongodb::{IndexModel, options::IndexOptions};
    let unique = |keys: Document| {
        IndexModel::builder()
            .keys(keys)
            .options(IndexOptions::builder().unique(true).build())
            .build()
    };
    let plain = |keys: Document| IndexModel::builder().keys(keys).build();
    store
        .raw(TASKS)
        .create_indexes([
            unique(doc! { "id": 1 }),
            unique(doc! { "dedupe_key": 1 }),
            plain(doc! { "status.state": 1, "id": -1 }),
            plain(doc! { "lease_holder": 1, "lease_until": 1 }),
            plain(doc! { "target.submission": 1, "id": -1 }),
        ])
        .await?;
    store
        .raw(CONTRIBUTIONS)
        .create_indexes([
            unique(doc! { "id": 1 }),
            plain(doc! { "task": 1 }),
            plain(doc! { "contributor": 1, "id": -1 }),
            plain(doc! { "kind": 1, "status.state": 1 }),
        ])
        .await?;
    store
        .raw(JUDGEMENTS)
        .create_index(plain(doc! { "submission": 1, "filed_at": 1 }))
        .await?;
    Ok(())
}

#[async_trait]
impl TaskStore for MongoStore {
    async fn insert_if_absent(&self, task: &Task) -> CoreResult<bool> {
        match self.raw(TASKS).insert_one(task_document(task)?).await {
            Ok(_) => Ok(true),
            Err(error) if is_duplicate_key(&error) => Ok(false),
            Err(error) => Err(unavailable(error)),
        }
    }

    async fn get(&self, id: &str) -> CoreResult<Option<Task>> {
        self.get_by_id(TASKS, id).await
    }

    async fn list(
        &self,
        filter: &TaskFilter,
        limit: u32,
        before: Option<String>,
    ) -> CoreResult<Listing<Task>> {
        let mut query = Document::new();
        if let Some(kind) = filter.kind {
            query.insert("kind", kind_value(kind));
        }
        if let Some(status) = &filter.status {
            query.insert("status.state", status.as_str());
        }
        if let Some(submission) = &filter.submission {
            query.insert("target.submission", submission.as_str());
        }
        if let Some(holder) = &filter.holder {
            query.insert("lease_holder", holder.as_str());
        }
        self.page(TASKS, query, limit, before).await
    }

    async fn replace(&self, task: &Task, expected: u64) -> CoreResult<()> {
        let mut document = task_document(task)?;
        document.insert("revision", (expected + 1) as i64);
        let result = self
            .raw(TASKS)
            .replace_one(
                doc! { "id": &task.id, "revision": expected as i64 },
                document,
            )
            .await
            .map_err(write_error)?;
        if result.matched_count == 1 {
            return Ok(());
        }
        match self
            .raw(TASKS)
            .find_one(doc! { "id": &task.id })
            .await
            .map_err(unavailable)?
        {
            Some(_) => Err(CoreError::StaleRevision {
                kind: "task",
                id: task.id.clone(),
            }),
            None => Err(CoreError::not_found("task", &task.id)),
        }
    }

    async fn active_leases(&self, holder: &PersonId, now: DateTime<Utc>) -> CoreResult<u32> {
        let count = self
            .raw(TASKS)
            .count_documents(doc! { "lease_holder": holder.as_str(), "lease_until": { "$gt": BsonDateTime::from_chrono(now) } })
            .await
            .map_err(unavailable)?;
        Ok(count as u32)
    }

    async fn expired_leases(&self, now: DateTime<Utc>, limit: u32) -> CoreResult<Vec<Task>> {
        let documents: Vec<Document> = self
            .raw(TASKS)
            .find(doc! { "status.state": "leased", "lease_until": { "$lte": BsonDateTime::from_chrono(now) } })
            .limit(i64::from(limit))
            .await
            .map_err(unavailable)?
            .try_collect()
            .await
            .map_err(unavailable)?;
        documents.into_iter().map(from_document).collect()
    }
}

#[async_trait]
impl ContributionStore for MongoStore {
    async fn insert(&self, contribution: &Contribution) -> CoreResult<()> {
        MongoStore::insert(self, CONTRIBUTIONS, contribution).await
    }

    async fn get(&self, id: &str) -> CoreResult<Option<Contribution>> {
        self.get_by_id(CONTRIBUTIONS, id).await
    }

    async fn list(
        &self,
        filter: &ContributionFilter,
        limit: u32,
        before: Option<String>,
    ) -> CoreResult<Listing<Contribution>> {
        let mut query = Document::new();
        if let Some(task) = &filter.task {
            query.insert("task", task.as_str());
        }
        if let Some(contributor) = &filter.contributor {
            query.insert("contributor", contributor.as_str());
        }
        if let Some(status) = &filter.status {
            query.insert("status.state", status.as_str());
        }
        if let Some(kind) = filter.kind {
            query.insert("kind", kind_value(kind));
        }
        self.page(CONTRIBUTIONS, query, limit, before).await
    }

    async fn replace(&self, contribution: &Contribution, expected: u64) -> CoreResult<()> {
        self.replace_revision(
            CONTRIBUTIONS,
            "contribution",
            &contribution.id,
            contribution,
            expected,
        )
        .await
    }

    async fn all_by(&self, contributor: Option<&PersonId>) -> CoreResult<Vec<Contribution>> {
        let query = match contributor {
            Some(person) => doc! { "contributor": person.as_str() },
            None => doc! {},
        };
        let documents: Vec<Document> = self
            .raw(CONTRIBUTIONS)
            .find(query)
            .await
            .map_err(unavailable)?
            .try_collect()
            .await
            .map_err(unavailable)?;
        documents.into_iter().map(from_document).collect()
    }
}

#[async_trait]
impl JudgementStore for MongoStore {
    async fn insert(&self, judgement: &ClaimJudgement) -> CoreResult<()> {
        MongoStore::insert(self, JUDGEMENTS, judgement).await
    }

    async fn for_submission(&self, submission: &SubmissionId) -> CoreResult<Vec<ClaimJudgement>> {
        let documents: Vec<Document> = self
            .raw(JUDGEMENTS)
            .find(doc! { "submission": submission.as_str() })
            .sort(doc! { "filed_at": 1 })
            .await
            .map_err(unavailable)?
            .try_collect()
            .await
            .map_err(unavailable)?;
        documents.into_iter().map(from_document).collect()
    }
}

#[async_trait]
impl GrantStore for MongoStore {
    async fn get(
        &self,
        donor: &PersonId,
    ) -> CoreResult<Option<wishpool_core::model::DonationGrant>> {
        let found = self
            .raw(GRANTS)
            .find_one(doc! { "donor": donor.as_str() })
            .await
            .map_err(unavailable)?;
        found.map(from_document).transpose()
    }

    async fn insert(&self, grant: &wishpool_core::model::DonationGrant) -> CoreResult<()> {
        MongoStore::insert(self, GRANTS, grant).await
    }

    async fn replace(
        &self,
        grant: &wishpool_core::model::DonationGrant,
        expected: u64,
    ) -> CoreResult<()> {
        let mut document = to_document(grant)?;
        document.insert("revision", (expected + 1) as i64);
        let result = self
            .raw(GRANTS)
            .replace_one(
                doc! { "donor": grant.donor.as_str(), "revision": expected as i64 },
                document,
            )
            .await
            .map_err(write_error)?;
        if result.matched_count == 1 {
            Ok(())
        } else {
            Err(CoreError::StaleRevision {
                kind: "grant",
                id: grant.donor.to_string(),
            })
        }
    }

    async fn active(&self) -> CoreResult<Vec<wishpool_core::model::DonationGrant>> {
        let documents: Vec<Document> = self
            .raw(GRANTS)
            .find(doc! { "status": "active" })
            .await
            .map_err(unavailable)?
            .try_collect()
            .await
            .map_err(unavailable)?;
        documents.into_iter().map(from_document).collect()
    }
}
