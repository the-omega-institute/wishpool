use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::PersonId;

/// Capabilities beyond those of any signed-in person. Every signed-in person
/// may post wishes and submit work; roles are granted by an administrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Triage wishes, file human stage reports, issue decisions.
    Editor,
    /// Attest the significance of a submission in their field.
    Endorser,
    /// A machine reviewer account that files stage reports.
    Reviewer,
    /// Grant roles.
    Admin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: PersonId,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orcid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affiliation: Option<String>,
    #[serde(default)]
    pub roles: BTreeSet<Role>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

impl Person {
    pub fn has(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }
}

/// Profile claims asserted by the identity provider at sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedIdentity {
    pub subject: PersonId,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub picture: Option<String>,
}

/// The authenticated principal of one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    pub person: PersonId,
    pub roles: BTreeSet<Role>,
}

impl Caller {
    pub fn from_person(person: &Person) -> Self {
        Self {
            person: person.id.clone(),
            roles: person.roles.clone(),
        }
    }

    pub fn has(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }

    pub fn require(&self, role: Role) -> crate::CoreResult<()> {
        if self.has(role) || self.has(Role::Admin) {
            Ok(())
        } else {
            Err(crate::CoreError::forbidden(format!(
                "requires the {role:?} role"
            )))
        }
    }
}
