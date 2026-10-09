//! Ports implemented outside Layer 2. Stores are the only authority for
//! state: a write that replaces an aggregate names the revision it read, and
//! a store refuses it if that revision has moved.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    CoreResult,
    ids::{PersonId, RecordId, SubmissionId},
    judgement::ClaimJudgement,
    model::{
        BlobRef, ClaimKind, Contribution, DonationGrant, Endorsement, Person, Record, RefereeFile,
        Role, Stage, Submission, Task, TaskKind, VerifiedIdentity,
    },
};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A page of results, newest first, with the cursor for the next page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listing<T> {
    pub items: Vec<T>,
    pub next_before: Option<String>,
}

#[async_trait]
pub trait PersonStore: Send + Sync {
    async fn upsert_sign_in(
        &self,
        identity: &VerifiedIdentity,
        at: DateTime<Utc>,
    ) -> CoreResult<Person>;
    async fn get(&self, id: &PersonId) -> CoreResult<Option<Person>>;
    async fn list(&self, limit: u32, before: Option<String>) -> CoreResult<Listing<Person>>;
    async fn set_roles(&self, id: &PersonId, roles: &[Role]) -> CoreResult<Person>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubmissionFilter {
    pub involving: Option<PersonId>,
    pub status: Option<String>,
}

#[async_trait]
pub trait SubmissionStore: Send + Sync {
    async fn insert(&self, submission: &Submission) -> CoreResult<()>;
    async fn get(&self, id: &SubmissionId) -> CoreResult<Option<Submission>>;
    async fn list(
        &self,
        filter: &SubmissionFilter,
        limit: u32,
        before: Option<String>,
    ) -> CoreResult<Listing<Submission>>;
    async fn replace(&self, submission: &Submission, expected: u64) -> CoreResult<()>;
    /// Draft and in-review papers submitted by `person`.
    async fn active_count(&self, person: &PersonId) -> CoreResult<u32>;
}

#[async_trait]
pub trait EndorsementStore: Send + Sync {
    async fn insert(&self, endorsement: &Endorsement) -> CoreResult<()>;
    async fn for_submission(&self, submission: &SubmissionId) -> CoreResult<Vec<Endorsement>>;
}

#[async_trait]
pub trait RecordStore: Send + Sync {
    async fn next_sequence(&self, year: i32) -> CoreResult<u64>;
    async fn insert(&self, record: &Record) -> CoreResult<()>;
    async fn get(&self, id: &RecordId) -> CoreResult<Option<Record>>;
    async fn list(&self, limit: u32, before: Option<String>) -> CoreResult<Listing<Record>>;
}

/// Opaque file storage for source archives and PDFs.
#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, bytes: &[u8], content_type: &str) -> CoreResult<BlobRef>;
    async fn get(&self, id: &str) -> CoreResult<Option<Vec<u8>>>;
}

/// One statement environment read from the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadStatement {
    pub kind: ClaimKind,
    pub display_name: String,
    pub title: Option<String>,
    pub latex_label: Option<String>,
    pub body: String,
    pub has_proof: bool,
    pub section: Option<String>,
}

/// What reading an uploaded LaTeX source yields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPaper {
    pub main_file: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub abstract_text: Option<String>,
    pub statements: Vec<ReadStatement>,
    /// Math macros the preamble defines, name → body.
    pub macros: std::collections::BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

/// Reads LaTeX sources (a `.tex`, `.zip` or `.tar.gz`).
pub trait PaperReader: Send + Sync {
    /// Read the structure; `Err` carries the reason for the author.
    fn read(&self, bytes: &[u8], filename: &str) -> Result<ReadPaper, String>;
}

/// Durable machine work: a review stage, compiling a version's PDF, or
/// advancing the current referee round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Stage(Stage),
    Compile,
    Referee,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewJob {
    pub submission: SubmissionId,
    pub kind: JobKind,
    pub attempts: u32,
}

#[async_trait]
pub trait ReviewQueue: Send + Sync {
    /// Enqueue unless an unfinished job of the same kind exists.
    async fn enqueue(&self, submission: &SubmissionId, kind: JobKind) -> CoreResult<()>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskFilter {
    pub kind: Option<TaskKind>,
    pub status: Option<String>,
    pub submission: Option<SubmissionId>,
    pub holder: Option<PersonId>,
}

#[async_trait]
pub trait TaskStore: Send + Sync {
    async fn insert_if_absent(&self, task: &Task) -> CoreResult<bool>;
    async fn get(&self, id: &str) -> CoreResult<Option<Task>>;
    async fn list(
        &self,
        filter: &TaskFilter,
        limit: u32,
        before: Option<String>,
    ) -> CoreResult<Listing<Task>>;
    async fn replace(&self, task: &Task, expected: u64) -> CoreResult<()>;
    async fn active_leases(&self, holder: &PersonId, now: DateTime<Utc>) -> CoreResult<u32>;
    async fn expired_leases(&self, now: DateTime<Utc>, limit: u32) -> CoreResult<Vec<Task>>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContributionFilter {
    pub task: Option<String>,
    pub contributor: Option<PersonId>,
    pub status: Option<String>,
    pub kind: Option<TaskKind>,
}

#[async_trait]
pub trait ContributionStore: Send + Sync {
    async fn insert(&self, contribution: &Contribution) -> CoreResult<()>;
    async fn get(&self, id: &str) -> CoreResult<Option<Contribution>>;
    async fn list(
        &self,
        filter: &ContributionFilter,
        limit: u32,
        before: Option<String>,
    ) -> CoreResult<Listing<Contribution>>;
    async fn replace(&self, contribution: &Contribution, expected: u64) -> CoreResult<()>;
    async fn all_by(&self, contributor: Option<&PersonId>) -> CoreResult<Vec<Contribution>>;
}

#[async_trait]
pub trait JudgementStore: Send + Sync {
    async fn insert(&self, judgement: &ClaimJudgement) -> CoreResult<()>;
    async fn for_submission(&self, submission: &SubmissionId) -> CoreResult<Vec<ClaimJudgement>>;
}

#[async_trait]
pub trait RefereeStore: Send + Sync {
    async fn get(&self, submission: &SubmissionId) -> CoreResult<Option<RefereeFile>>;
    async fn insert(&self, file: &RefereeFile) -> CoreResult<()>;
    async fn replace(&self, file: &RefereeFile, expected: u64) -> CoreResult<()>;
}

#[async_trait]
pub trait GrantStore: Send + Sync {
    async fn get(&self, donor: &PersonId) -> CoreResult<Option<DonationGrant>>;
    async fn insert(&self, grant: &DonationGrant) -> CoreResult<()>;
    async fn replace(&self, grant: &DonationGrant, expected: u64) -> CoreResult<()>;
    async fn active(&self) -> CoreResult<Vec<DonationGrant>>;
}

/// Everything Layer 2 needs, bundled for composition.
pub struct Ports {
    pub clock: Arc<dyn Clock>,
    pub people: Arc<dyn PersonStore>,
    pub submissions: Arc<dyn SubmissionStore>,
    pub endorsements: Arc<dyn EndorsementStore>,
    pub records: Arc<dyn RecordStore>,
    pub blobs: Arc<dyn BlobStore>,
    pub reader: Arc<dyn PaperReader>,
    pub queue: Arc<dyn ReviewQueue>,
    pub tasks: Arc<dyn TaskStore>,
    pub contributions: Arc<dyn ContributionStore>,
    pub judgements: Arc<dyn JudgementStore>,
    pub grants: Arc<dyn GrantStore>,
    pub referees: Arc<dyn RefereeStore>,
}
