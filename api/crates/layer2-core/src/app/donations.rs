//! Donated quota: the donor's view and the hosted worker's accounting.

use super::App;
use crate::{
    CoreError, CoreResult,
    ids::PersonId,
    model::{Caller, DonationGrant, GrantStatus, period_of},
};

impl App {
    pub async fn donation(&self, caller: &Caller) -> CoreResult<Option<DonationGrant>> {
        self.ports.grants.get(&caller.person).await
    }

    /// The donor changes their cap or pauses, resumes or revokes the grant.
    pub async fn update_donation(
        &self,
        caller: &Caller,
        monthly_cap: Option<u64>,
        status: Option<GrantStatus>,
    ) -> CoreResult<DonationGrant> {
        let mut grant = self
            .ports
            .grants
            .get(&caller.person)
            .await?
            .ok_or_else(|| CoreError::not_found("donation", caller.person.as_str()))?;
        if grant.status == GrantStatus::Revoked {
            return Err(CoreError::conflict(
                "a revoked grant is renewed by authorizing again",
            ));
        }
        if let Some(cap) = monthly_cap {
            DonationGrant::validate_cap(cap)?;
            grant.monthly_cap = cap;
        }
        if let Some(status) = status {
            grant.status = status;
        }
        let expected = grant.revision;
        grant.updated_at = self.ports.clock.now();
        grant.revision = expected + 1;
        self.ports.grants.replace(&grant, expected).await?;
        Ok(grant)
    }

    /// Composition-only: record a grant after the donor authorized the
    /// venue in NyxID and the delegated credential was stored.
    pub async fn create_or_renew_donation(
        &self,
        donor: &PersonId,
        monthly_cap: u64,
        model: &str,
    ) -> CoreResult<DonationGrant> {
        DonationGrant::validate_cap(monthly_cap)?;
        let now = self.ports.clock.now();
        match self.ports.grants.get(donor).await? {
            Some(mut grant) => {
                let expected = grant.revision;
                grant.monthly_cap = monthly_cap;
                grant.model = model.to_owned();
                grant.status = GrantStatus::Active;
                grant.updated_at = now;
                grant.revision = expected + 1;
                self.ports.grants.replace(&grant, expected).await?;
                Ok(grant)
            }
            None => {
                let grant = DonationGrant {
                    donor: donor.clone(),
                    monthly_cap,
                    model: model.to_owned(),
                    period: period_of(now),
                    used: 0,
                    status: GrantStatus::Active,
                    created_at: now,
                    updated_at: now,
                    revision: 0,
                };
                self.ports.grants.insert(&grant).await?;
                Ok(grant)
            }
        }
    }

    /// Composition-only: grants with budget left this month.
    pub async fn spendable_donations(&self) -> CoreResult<Vec<DonationGrant>> {
        let now = self.ports.clock.now();
        Ok(self
            .ports
            .grants
            .active()
            .await?
            .into_iter()
            .filter(|g| g.remaining(now) > 0)
            .collect())
    }

    /// Composition-only: charge metered usage to a grant.
    pub async fn charge_donation(
        &self,
        donor: &PersonId,
        tokens: u64,
    ) -> CoreResult<DonationGrant> {
        for _ in 0..3 {
            let mut grant = self
                .ports
                .grants
                .get(donor)
                .await?
                .ok_or_else(|| CoreError::not_found("donation", donor.as_str()))?;
            let expected = grant.revision;
            grant.charge(tokens, self.ports.clock.now());
            grant.revision = expected + 1;
            match self.ports.grants.replace(&grant, expected).await {
                Ok(()) => return Ok(grant),
                Err(CoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(CoreError::Unavailable(
            "could not record usage after three attempts".into(),
        ))
    }

    /// Composition-only: revoke when the delegated credential stops working.
    pub async fn revoke_donation(&self, donor: &PersonId) -> CoreResult<()> {
        if let Some(mut grant) = self.ports.grants.get(donor).await? {
            let expected = grant.revision;
            grant.status = GrantStatus::Revoked;
            grant.updated_at = self.ports.clock.now();
            grant.revision = expected + 1;
            self.ports.grants.replace(&grant, expected).await?;
        }
        Ok(())
    }
}
