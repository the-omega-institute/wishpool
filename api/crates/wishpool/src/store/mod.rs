//! MongoDB persistence. The database is the only authority for state; every
//! aggregate write is fenced by its revision.
//!
//! Domain aggregates are stored as their serde form plus Mongo's own `_id`;
//! lookups use the unique `id` field. Short-lived authentication documents
//! and review jobs keep BSON dates so TTL indexes can expire them.

mod auth;
mod blobs;
mod domain;
mod jobs;
mod network;
mod solving;

use futures::TryStreamExt;
use mongodb::{
    Client, Collection, Database, IndexModel,
    bson::{Document, doc},
    error::{Error as MongoError, ErrorKind, WriteFailure},
    options::IndexOptions,
};
use serde::{Serialize, de::DeserializeOwned};
use wishpool_core::{CoreError, CoreResult, ports::Listing};

pub(crate) const PEOPLE: &str = "people";
pub(crate) const SUBMISSIONS: &str = "submissions";
pub(crate) const ENDORSEMENTS: &str = "endorsements";
pub(crate) const RECORDS: &str = "records";
pub(crate) const COUNTERS: &str = "counters";
pub(crate) const SESSIONS: &str = "sessions";
pub(crate) const LOGIN_ATTEMPTS: &str = "login_attempts";
pub(crate) const REFEREE_FILES: &str = "referee_files";
pub(crate) const REVIEW_JOBS: &str = "review_jobs";

#[derive(Clone)]
pub struct MongoStore {
    client: Client,
    db: Database,
}

impl MongoStore {
    pub async fn connect(uri: &str, database: &str) -> anyhow::Result<Self> {
        let client = Client::with_uri_str(uri).await?;
        let db = client.database(database);
        db.run_command(doc! { "ping": 1 }).await?;
        let store = Self { client, db };
        store.ensure_indexes().await?;
        Ok(store)
    }

    /// Close the connection pool before the runtime stops; dropping a live
    /// pool during runtime shutdown makes its background tasks panic.
    pub async fn shutdown(self) {
        self.client.shutdown().await;
    }

    pub async fn ping(&self) -> CoreResult<()> {
        self.db
            .run_command(doc! { "ping": 1 })
            .await
            .map(|_| ())
            .map_err(unavailable)
    }

    async fn ensure_indexes(&self) -> anyhow::Result<()> {
        let unique = |keys: Document| {
            IndexModel::builder()
                .keys(keys)
                .options(IndexOptions::builder().unique(true).build())
                .build()
        };
        let plain = |keys: Document| IndexModel::builder().keys(keys).build();
        let ttl = |field: &str| {
            IndexModel::builder()
                .keys(doc! { field: 1 })
                .options(
                    IndexOptions::builder()
                        .expire_after(std::time::Duration::ZERO)
                        .build(),
                )
                .build()
        };
        self.raw(PEOPLE)
            .create_index(unique(doc! { "id": 1 }))
            .await?;
        self.raw(SUBMISSIONS)
            .create_indexes([
                unique(doc! { "id": 1 }),
                plain(doc! { "status.state": 1, "id": -1 }),
                plain(doc! { "submitter": 1, "id": -1 }),
                plain(doc! { "authors.person": 1, "id": -1 }),
            ])
            .await?;
        self.raw(ENDORSEMENTS)
            .create_index(unique(doc! { "submission": 1, "endorser": 1 }))
            .await?;
        self.raw(RECORDS)
            .create_indexes([unique(doc! { "id": 1 }), unique(doc! { "submission": 1 })])
            .await?;
        self.raw(SESSIONS).create_index(ttl("expires_at")).await?;
        self.raw(LOGIN_ATTEMPTS)
            .create_index(ttl("expires_at"))
            .await?;
        self.raw(REVIEW_JOBS)
            .create_index(plain(doc! { "state": 1, "available_at": 1 }))
            .await?;
        self.raw(REFEREE_FILES)
            .create_index(unique(doc! { "id": 1 }))
            .await?;
        network::ensure_indexes(self).await?;
        solving::ensure_indexes(self).await?;
        Ok(())
    }

    /// Source archives and PDFs exceed the 16 MB document limit; they live
    /// in GridFS.
    pub(crate) fn bucket(&self) -> mongodb::gridfs::GridFsBucket {
        self.db.gridfs_bucket(
            mongodb::options::GridFsBucketOptions::builder()
                .bucket_name("papers".to_owned())
                .build(),
        )
    }

    pub fn raw(&self, name: &str) -> Collection<Document> {
        self.db.collection(name)
    }

    /// Fetch one aggregate by its `id`.
    pub(crate) async fn get_by_id<T: DeserializeOwned>(
        &self,
        collection: &str,
        id: &str,
    ) -> CoreResult<Option<T>> {
        let found = self
            .raw(collection)
            .find_one(doc! { "id": id })
            .await
            .map_err(unavailable)?;
        found.map(from_document).transpose()
    }

    pub(crate) async fn insert<T: Serialize>(&self, collection: &str, value: &T) -> CoreResult<()> {
        self.raw(collection)
            .insert_one(to_document(value)?)
            .await
            .map_err(write_error)?;
        Ok(())
    }

    /// Replace the aggregate with `id` if its stored revision is `expected`.
    pub(crate) async fn replace_revision<T: Serialize>(
        &self,
        collection: &str,
        kind: &'static str,
        id: &str,
        value: &T,
        expected: u64,
    ) -> CoreResult<()> {
        let mut document = to_document(value)?;
        document.insert("revision", (expected + 1) as i64);
        let result = self
            .raw(collection)
            .replace_one(doc! { "id": id, "revision": expected as i64 }, document)
            .await
            .map_err(write_error)?;
        if result.matched_count == 1 {
            return Ok(());
        }
        match self
            .raw(collection)
            .find_one(doc! { "id": id })
            .await
            .map_err(unavailable)?
        {
            Some(_) => Err(CoreError::StaleRevision {
                kind,
                id: id.to_owned(),
            }),
            None => Err(CoreError::not_found(kind, id)),
        }
    }

    /// Newest-first page over `filter`, using `id` as the cursor.
    pub(crate) async fn page<T: DeserializeOwned>(
        &self,
        collection: &str,
        mut filter: Document,
        limit: u32,
        before: Option<String>,
    ) -> CoreResult<Listing<T>> {
        if let Some(before) = before {
            filter.insert("id", doc! { "$lt": before });
        }
        let mut documents: Vec<Document> = self
            .raw(collection)
            .find(filter)
            .sort(doc! { "id": -1 })
            .limit(i64::from(limit) + 1)
            .await
            .map_err(unavailable)?
            .try_collect()
            .await
            .map_err(unavailable)?;
        // One extra document was read to learn whether another page exists;
        // the cursor is the id of the last document returned.
        let next_before = if documents.len() > limit as usize {
            documents.pop();
            documents
                .last()
                .and_then(|d| d.get_str("id").ok())
                .map(str::to_owned)
        } else {
            None
        };
        let items = documents
            .into_iter()
            .map(from_document)
            .collect::<CoreResult<Vec<T>>>()?;
        Ok(Listing { items, next_before })
    }
}

pub(crate) fn to_document<T: Serialize>(value: &T) -> CoreResult<Document> {
    mongodb::bson::to_document(value)
        .map_err(|e| CoreError::Unavailable(format!("encode document: {e}")))
}

pub(crate) fn from_document<T: DeserializeOwned>(document: Document) -> CoreResult<T> {
    mongodb::bson::from_document(document)
        .map_err(|e| CoreError::Unavailable(format!("decode document: {e}")))
}

pub fn unavailable(error: MongoError) -> CoreError {
    CoreError::Unavailable(format!("database: {error}"))
}

pub(crate) fn is_duplicate_key(error: &MongoError) -> bool {
    matches!(error.kind.as_ref(), ErrorKind::Write(WriteFailure::WriteError(e)) if e.code == 11000)
        || matches!(error.kind.as_ref(), ErrorKind::Command(e) if e.code == 11000)
}

pub fn write_error(error: MongoError) -> CoreError {
    if is_duplicate_key(&error) {
        CoreError::conflict("a document with the same identity already exists")
    } else {
        unavailable(error)
    }
}

#[cfg(test)]
mod tests;
