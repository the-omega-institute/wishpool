//! Donated model quota: a contributor lets the venue spend part of the model
//! credentials they already broker in NyxID on hosted tasks. Only the
//! non-secret part lives here; the delegated credential is held encrypted by
//! the binary.

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};

use crate::{CoreError, CoreResult, ids::PersonId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantStatus {
    Active,
    Paused,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DonationGrant {
    pub donor: PersonId,
    /// Tokens (input + output) the venue may spend per calendar month.
    pub monthly_cap: u64,
    /// The model the donor's quota is spent on, routed by the NyxID gateway.
    pub model: String,
    /// `YYYY-MM` of `used`.
    pub period: String,
    pub used: u64,
    pub status: GrantStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
}

pub const MAX_MONTHLY_CAP: u64 = 50_000_000;

pub fn period_of(at: DateTime<Utc>) -> String {
    format!("{:04}-{:02}", at.year(), at.month())
}

impl DonationGrant {
    pub fn validate_cap(cap: u64) -> CoreResult<()> {
        if cap == 0 || cap > MAX_MONTHLY_CAP {
            return Err(CoreError::invalid(format!(
                "the monthly cap must be between 1 and {MAX_MONTHLY_CAP} tokens"
            )));
        }
        Ok(())
    }

    /// Tokens still available this month.
    pub fn remaining(&self, now: DateTime<Utc>) -> u64 {
        if self.status != GrantStatus::Active {
            return 0;
        }
        if self.period != period_of(now) {
            return self.monthly_cap;
        }
        self.monthly_cap.saturating_sub(self.used)
    }

    /// Record spent tokens, rolling the period over when the month changed.
    pub fn charge(&mut self, tokens: u64, now: DateTime<Utc>) {
        let period = period_of(now);
        if self.period != period {
            self.period = period;
            self.used = 0;
        }
        self.used = self.used.saturating_add(tokens);
        self.updated_at = now;
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn budget_rolls_over_monthly() {
        let october = Utc.with_ymd_and_hms(2026, 10, 8, 0, 0, 0).unwrap();
        let november = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        let mut grant = DonationGrant {
            donor: "d".into(),
            monthly_cap: 1_000,
            model: "claude-opus-5-5".into(),
            period: period_of(october),
            used: 0,
            status: GrantStatus::Active,
            created_at: october,
            updated_at: october,
            revision: 0,
        };
        grant.charge(900, october);
        assert_eq!(grant.remaining(october), 100);
        assert_eq!(grant.remaining(november), 1_000);
        grant.charge(10, november);
        assert_eq!((grant.period.as_str(), grant.used), ("2026-11", 10));
        grant.status = GrantStatus::Paused;
        assert_eq!(grant.remaining(november), 0);
        assert!(DonationGrant::validate_cap(0).is_err());
    }
}
