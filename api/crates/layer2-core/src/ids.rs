//! Typed identifiers. Entity ids are UUIDv7 strings, so lexical order equals
//! creation order; a person's id is the identity provider's `sub` claim.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }
    };
}

string_id!(
    /// The identity provider subject (`sub`) of a signed-in person.
    PersonId
);
string_id!(SubmissionId);
string_id!(EndorsementId);
string_id!(
    /// Public record number, `WP-<year>-<4-digit sequence>`.
    RecordId
);
string_id!(
    /// Claim label local to one submission, e.g. `C1`.
    ClaimId
);

pub fn new_uuid() -> String {
    uuid::Uuid::now_v7().to_string()
}

impl RecordId {
    pub fn format(year: i32, sequence: u64) -> Self {
        Self(format!("WP-{year}-{sequence:04}"))
    }
}
