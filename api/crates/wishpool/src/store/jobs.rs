use async_trait::async_trait;
use chrono::Utc;
use mongodb::{
    bson::{DateTime as BsonDateTime, doc},
    options::ReturnDocument,
};
use wishpool_core::{
    CoreError, CoreResult,
    ids::SubmissionId,
    ports::{JobKind, ReviewQueue},
};

use super::{MongoStore, REVIEW_JOBS, is_duplicate_key, unavailable};
use crate::{
    auth,
    review::{JobLease, LeasedJob, MAX_ATTEMPTS, kind_key, parse_kind_key},
};

const LEASE: chrono::Duration = chrono::Duration::minutes(30);

fn job_id(submission: &SubmissionId, kind: JobKind) -> String {
    format!("{submission}:{}", kind_key(kind))
}

#[async_trait]
impl ReviewQueue for MongoStore {
    async fn enqueue(&self, submission: &SubmissionId, kind: JobKind) -> CoreResult<()> {
        let now = BsonDateTime::now();
        let result = self
            .raw(REVIEW_JOBS)
            .update_one(
                // A leased job is in flight; anything else is (re)queued.
                doc! { "_id": job_id(submission, kind), "state": { "$ne": "leased" } },
                doc! {
                    "$set": { "state": "queued", "available_at": now, "attempts": 0_i32, "last_error": null },
                    "$setOnInsert": { "submission": submission.as_str(), "kind": kind_key(kind), "created_at": now },
                },
            )
            .upsert(true)
            .await;
        match result {
            Ok(_) => Ok(()),
            // The upsert collided with the leased job's _id: already in flight.
            Err(error) if is_duplicate_key(&error) => Ok(()),
            Err(error) => Err(unavailable(error)),
        }
    }
}

#[async_trait]
impl JobLease for MongoStore {
    async fn claim(&self) -> CoreResult<Option<LeasedJob>> {
        let now = Utc::now();
        let lease = auth::random_token();
        let found = self
            .raw(REVIEW_JOBS)
            .find_one_and_update(
                doc! {
                    "$or": [
                        { "state": "queued", "available_at": { "$lte": BsonDateTime::from_chrono(now) } },
                        { "state": "leased", "lease_until": { "$lte": BsonDateTime::from_chrono(now) } },
                    ]
                },
                doc! {
                    "$set": { "state": "leased", "lease": &lease, "lease_until": BsonDateTime::from_chrono(now + LEASE) },
                    "$inc": { "attempts": 1_i32 },
                },
            )
            .sort(doc! { "available_at": 1 })
            .return_document(ReturnDocument::After)
            .await
            .map_err(unavailable)?;
        let Some(document) = found else {
            return Ok(None);
        };
        let field = |name: &str| {
            document
                .get_str(name)
                .map(str::to_owned)
                .map_err(|_| CoreError::Unavailable(format!("review job lacks {name}")))
        };
        let key = field("kind")?;
        let kind = parse_kind_key(&key)
            .ok_or_else(|| CoreError::Unavailable(format!("review job kind {key:?}")))?;
        Ok(Some(LeasedJob {
            submission: SubmissionId(field("submission")?),
            kind,
            attempts: document.get_i32("attempts").unwrap_or(1).max(0) as u32,
            lease,
        }))
    }

    async fn renew(&self, job: &LeasedJob) -> CoreResult<bool> {
        let now = Utc::now();
        let result = self
            .raw(REVIEW_JOBS)
            .update_one(
                doc! {
                    "_id": job_id(&job.submission, job.kind),
                    "state": "leased",
                    "lease": &job.lease,
                    // Once the lease has expired, another worker may claim it;
                    // the old token must not be able to revive it.
                    "lease_until": { "$gt": BsonDateTime::from_chrono(now) },
                },
                doc! {
                    "$set": { "lease_until": BsonDateTime::from_chrono(now + LEASE) },
                },
            )
            .await
            .map_err(unavailable)?;
        Ok(result.matched_count == 1)
    }

    async fn complete(&self, job: &LeasedJob) -> CoreResult<()> {
        self.raw(REVIEW_JOBS)
            .delete_one(doc! { "_id": job_id(&job.submission, job.kind), "state": "leased", "lease": &job.lease, "lease_until": { "$gt": BsonDateTime::now() } })
            .await
            .map_err(unavailable)?;
        Ok(())
    }

    async fn defer(&self, job: &LeasedJob, delay: std::time::Duration) -> CoreResult<()> {
        let delay = chrono::Duration::from_std(delay)
            .map_err(|_| CoreError::invalid("job deferral is too long"))?;
        self.raw(REVIEW_JOBS).update_one(
            doc! { "_id": job_id(&job.submission, job.kind), "state": "leased", "lease": &job.lease, "lease_until": { "$gt": BsonDateTime::now() } },
            doc! {
                "$set": { "state": "queued", "available_at": BsonDateTime::from_chrono(Utc::now() + delay) },
                "$inc": { "attempts": -1_i32 },
                "$unset": { "lease": "", "lease_until": "" },
            },
        ).await.map_err(unavailable)?;
        Ok(())
    }

    async fn retry(&self, job: &LeasedJob, error: &str) -> CoreResult<()> {
        let (state, delay) = if job.attempts >= MAX_ATTEMPTS {
            ("failed", chrono::Duration::zero())
        } else {
            (
                "queued",
                chrono::Duration::seconds(30 * i64::from(job.attempts)),
            )
        };
        self.raw(REVIEW_JOBS)
            .update_one(
                doc! { "_id": job_id(&job.submission, job.kind), "state": "leased", "lease": &job.lease, "lease_until": { "$gt": BsonDateTime::now() } },
                doc! {
                    "$set": {
                        "state": state,
                        "available_at": BsonDateTime::from_chrono(Utc::now() + delay),
                        "last_error": error.chars().take(2_000).collect::<String>(),
                    },
                    "$unset": { "lease": "", "lease_until": "" },
                },
            )
            .await
            .map_err(unavailable)?;
        Ok(())
    }
}
