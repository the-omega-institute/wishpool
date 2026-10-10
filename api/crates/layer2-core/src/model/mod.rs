//! Domain aggregates. Every aggregate with a lifecycle carries a `revision`
//! used for optimistic concurrency; stores must reject a write whose expected
//! revision is not the stored one.

pub(crate) mod claim;
mod common;
mod conjecture;
mod donation;
mod endorsement;
mod formalization;
mod paper;
mod person;
mod record;
pub mod referee;
mod review;
mod solving;
mod task;

pub use claim::*;
pub use common::*;
pub use conjecture::*;
pub use donation::*;
pub use endorsement::*;
pub use formalization::*;
pub use paper::*;
pub use person::*;
pub use record::*;
pub use referee::{
    Advice, AuditedClaim, AuditedConcern, ConcernStatus, Correctness, Effort,
    Evidence as ImprovementEvidence, Feasibility, FeedbackLetter, FormalAttempt, FormalProbe,
    FormalProbeView, FormalizationCandidate, Improvement, ImprovementKind, LetterDraft,
    MAX_LETTER_CHARS, NewLetter, ProbeOutcome, ProbeOutcomeView, Recommendation, RefereeAudit,
    RefereeClaim, RefereeConcern, RefereeFile, RefereeReport, RefereeRound, RefereeRoundView,
    RefereeView, RoundUpdate, Severity, Step, StepState,
};
pub use review::*;
pub use solving::*;
pub use task::*;
