use async_trait::async_trait;
use mongodb::bson::{DateTime as BsonDateTime, doc};
use wishpool_core::{CoreError, CoreResult, ids::PersonId};

use super::{LOGIN_ATTEMPTS, MongoStore, SESSIONS, unavailable, write_error};
use crate::auth::{AuthStore, LoginAttempt, SessionRecord};

fn decode_error(field: &str) -> CoreError {
    CoreError::Unavailable(format!("stored auth document lacks {field}"))
}

#[async_trait]
impl AuthStore for MongoStore {
    async fn create_session(&self, digest: &str, session: &SessionRecord) -> CoreResult<()> {
        self.raw(SESSIONS)
            .insert_one(doc! {
                "_id": digest,
                "subject": session.subject.as_str(),
                "expires_at": BsonDateTime::from_chrono(session.expires_at),
            })
            .await
            .map_err(write_error)?;
        Ok(())
    }

    async fn session(&self, digest: &str) -> CoreResult<Option<SessionRecord>> {
        // TTL deletion runs about once a minute; expiry is also checked here.
        let now = BsonDateTime::now();
        let found = self
            .raw(SESSIONS)
            .find_one(doc! { "_id": digest, "expires_at": { "$gt": now } })
            .await
            .map_err(unavailable)?;
        found
            .map(|d| {
                Ok(SessionRecord {
                    subject: PersonId(
                        d.get_str("subject")
                            .map_err(|_| decode_error("subject"))?
                            .to_owned(),
                    ),
                    expires_at: d
                        .get_datetime("expires_at")
                        .map_err(|_| decode_error("expires_at"))?
                        .to_chrono(),
                })
            })
            .transpose()
    }

    async fn delete_session(&self, digest: &str) -> CoreResult<()> {
        self.raw(SESSIONS)
            .delete_one(doc! { "_id": digest })
            .await
            .map_err(unavailable)?;
        Ok(())
    }

    async fn put_attempt(&self, state_digest: &str, attempt: &LoginAttempt) -> CoreResult<()> {
        self.raw(LOGIN_ATTEMPTS)
            .insert_one(doc! {
                "_id": state_digest,
                "binding": &attempt.binding,
                "verifier": &attempt.verifier,
                "nonce": &attempt.nonce,
                "return_to": &attempt.return_to,
                "expires_at": BsonDateTime::from_chrono(attempt.expires_at),
                "donation_donor": attempt.donation.as_ref().map(|d| d.donor.0.clone()),
                "donation_cap": attempt.donation.as_ref().map(|d| d.monthly_cap as i64),
                "donation_model": attempt.donation.as_ref().map(|d| d.model.clone()),
            })
            .await
            .map_err(write_error)?;
        Ok(())
    }

    async fn take_attempt(&self, state_digest: &str) -> CoreResult<Option<LoginAttempt>> {
        let found = self
            .raw(LOGIN_ATTEMPTS)
            .find_one_and_delete(doc! { "_id": state_digest })
            .await
            .map_err(unavailable)?;
        found
            .map(|d| {
                let text = |field: &str| {
                    d.get_str(field)
                        .map(str::to_owned)
                        .map_err(|_| decode_error(field))
                };
                Ok(LoginAttempt {
                    binding: text("binding")?,
                    verifier: text("verifier")?,
                    nonce: text("nonce")?,
                    return_to: text("return_to")?,
                    expires_at: d
                        .get_datetime("expires_at")
                        .map_err(|_| decode_error("expires_at"))?
                        .to_chrono(),
                    donation: match (
                        d.get_str("donation_donor"),
                        d.get_i64("donation_cap"),
                        d.get_str("donation_model"),
                    ) {
                        (Ok(donor), Ok(cap), Ok(model)) => Some(crate::auth::DonationRequest {
                            donor: PersonId(donor.to_owned()),
                            monthly_cap: cap.max(0) as u64,
                            model: model.to_owned(),
                        }),
                        _ => None,
                    },
                })
            })
            .transpose()
    }
}
