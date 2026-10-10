//! Layer 2 store ports over MongoDB.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use mongodb::{
    bson::{Document, doc, to_bson},
    options::ReturnDocument,
};
use wishpool_core::{
    CoreError, CoreResult,
    ids::{PersonId, RecordId, SubmissionId},
    memory::display_name,
    model::{Endorsement, Person, Record, Role, Submission, VerifiedIdentity},
    ports::*,
};

use super::*;

fn bson_value<T: serde::Serialize>(value: &T) -> CoreResult<mongodb::bson::Bson> {
    to_bson(value).map_err(|e| CoreError::Unavailable(format!("encode value: {e}")))
}

#[async_trait]
impl PersonStore for MongoStore {
    async fn upsert_sign_in(
        &self,
        identity: &VerifiedIdentity,
        at: DateTime<Utc>,
    ) -> CoreResult<Person> {
        let at = bson_value(&at)?;
        let mut set = doc! { "display_name": display_name(identity), "last_seen_at": at.clone() };
        let mut unset = Document::new();
        for (field, value) in [("email", &identity.email), ("picture", &identity.picture)] {
            match value {
                Some(value) => set.insert(field, value),
                None => unset.insert(field, ""),
            };
        }
        let mut update = doc! {
            "$set": set,
            "$setOnInsert": { "id": identity.subject.as_str(), "roles": [], "created_at": at },
        };
        if !unset.is_empty() {
            update.insert("$unset", unset);
        }
        let person = self
            .raw(PEOPLE)
            .find_one_and_update(doc! { "id": identity.subject.as_str() }, update)
            .upsert(true)
            .return_document(ReturnDocument::After)
            .await
            .map_err(write_error)?
            .ok_or_else(|| CoreError::Unavailable("upsert returned no person".into()))?;
        from_document(person)
    }

    async fn get(&self, id: &PersonId) -> CoreResult<Option<Person>> {
        self.get_by_id(PEOPLE, id.as_str()).await
    }

    async fn list(&self, limit: u32, before: Option<String>) -> CoreResult<Listing<Person>> {
        self.page(PEOPLE, doc! {}, limit, before).await
    }

    async fn set_roles(&self, id: &PersonId, roles: &[Role]) -> CoreResult<Person> {
        let mut roles: Vec<Role> = roles.to_vec();
        roles.sort();
        roles.dedup();
        let person = self
            .raw(PEOPLE)
            .find_one_and_update(
                doc! { "id": id.as_str() },
                doc! { "$set": { "roles": bson_value(&roles)? } },
            )
            .return_document(ReturnDocument::After)
            .await
            .map_err(unavailable)?
            .ok_or_else(|| CoreError::not_found("person", id.as_str()))?;
        from_document(person)
    }
}

#[async_trait]
impl SubmissionStore for MongoStore {
    async fn insert(&self, submission: &Submission) -> CoreResult<()> {
        MongoStore::insert(self, SUBMISSIONS, submission).await
    }

    async fn get(&self, id: &SubmissionId) -> CoreResult<Option<Submission>> {
        self.get_by_id(SUBMISSIONS, id.as_str()).await
    }

    async fn list(
        &self,
        filter: &SubmissionFilter,
        limit: u32,
        before: Option<String>,
    ) -> CoreResult<Listing<Submission>> {
        let mut query = Document::new();
        if let Some(person) = &filter.involving {
            query.insert(
                "$or",
                vec![
                    doc! { "submitter": person.as_str() },
                    doc! { "authors.person": person.as_str() },
                ],
            );
        }
        if let Some(status) = &filter.status {
            query.insert("status.state", status.as_str());
        }
        self.page(SUBMISSIONS, query, limit, before).await
    }

    async fn active_count(&self, person: &PersonId) -> CoreResult<u32> {
        let n = self
            .raw(SUBMISSIONS)
            .count_documents(doc! {
                "submitter": person.as_str(),
                "status.state": { "$in": ["draft", "in_review"] },
            })
            .await
            .map_err(unavailable)?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }

    async fn replace(&self, submission: &Submission, expected: u64) -> CoreResult<()> {
        self.replace_revision(
            SUBMISSIONS,
            "submission",
            submission.id.as_str(),
            submission,
            expected,
        )
        .await
    }
}

#[async_trait]
impl EndorsementStore for MongoStore {
    async fn insert(&self, endorsement: &Endorsement) -> CoreResult<()> {
        match MongoStore::insert(self, ENDORSEMENTS, endorsement).await {
            Err(CoreError::Conflict(_)) => Err(CoreError::conflict(
                "this endorser has already endorsed the submission",
            )),
            other => other,
        }
    }

    async fn for_submission(&self, submission: &SubmissionId) -> CoreResult<Vec<Endorsement>> {
        let documents: Vec<Document> = self
            .raw(ENDORSEMENTS)
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
impl RecordStore for MongoStore {
    async fn for_submission(&self, id: &SubmissionId) -> CoreResult<Option<Record>> {
        self.raw(RECORDS)
            .find_one(doc! { "submission": id.as_str() })
            .await
            .map_err(unavailable)?
            .map(from_document)
            .transpose()
    }
    async fn next_sequence(&self, year: i32) -> CoreResult<u64> {
        let counter = self
            .raw(COUNTERS)
            .find_one_and_update(
                doc! { "_id": format!("records-{year}") },
                doc! { "$inc": { "value": 1_i64 } },
            )
            .upsert(true)
            .return_document(ReturnDocument::After)
            .await
            .map_err(write_error)?
            .ok_or_else(|| CoreError::Unavailable("counter upsert returned nothing".into()))?;
        counter
            .get_i64("value")
            .map(|v| v as u64)
            .map_err(|e| CoreError::Unavailable(format!("counter value: {e}")))
    }

    async fn insert(&self, record: &Record) -> CoreResult<()> {
        MongoStore::insert(self, RECORDS, record).await
    }

    async fn get(&self, id: &RecordId) -> CoreResult<Option<Record>> {
        self.get_by_id(RECORDS, id.as_str()).await
    }

    async fn list(&self, limit: u32, before: Option<String>) -> CoreResult<Listing<Record>> {
        self.page(RECORDS, doc! {}, limit, before).await
    }
}

#[async_trait]
impl RefereeStore for MongoStore {
    async fn get(
        &self,
        id: &SubmissionId,
    ) -> CoreResult<Option<wishpool_core::model::RefereeFile>> {
        self.get_by_id(REFEREE_FILES, id.as_str()).await
    }

    async fn insert(&self, file: &wishpool_core::model::RefereeFile) -> CoreResult<()> {
        MongoStore::insert(self, REFEREE_FILES, file).await
    }

    async fn replace(
        &self,
        file: &wishpool_core::model::RefereeFile,
        expected: u64,
    ) -> CoreResult<()> {
        self.replace_revision(
            REFEREE_FILES,
            "referee file",
            file.id.as_str(),
            file,
            expected,
        )
        .await
    }
}
