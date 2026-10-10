//! In-memory implementations of every store, with the same revision and
//! uniqueness semantics as MongoDB. Used by tests in every layer and by the
//! `memory` storage mode for local work.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;

use crate::{
    CoreError, CoreResult,
    ids::{PersonId, RecordId, SubmissionId},
    judgement::ClaimJudgement,
    model::*,
    ports::*,
};

fn page<T: Clone>(
    items: &BTreeMap<String, T>,
    limit: u32,
    before: Option<String>,
    keep: impl Fn(&T) -> bool,
) -> Listing<T> {
    let mut out: Vec<(String, T)> = items
        .iter()
        .rev()
        .filter(|(id, _)| before.as_ref().is_none_or(|b| id.as_str() < b.as_str()))
        .filter(|(_, v)| keep(v))
        .take(limit as usize + 1)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let next_before = if out.len() > limit as usize {
        out.pop();
        out.last().map(|(k, _)| k.clone())
    } else {
        None
    };
    Listing {
        items: out.into_iter().map(|(_, v)| v).collect(),
        next_before,
    }
}

/// The name shown for a person: the provider's name, else the email's local
/// part, else the subject.
pub fn display_name(identity: &VerifiedIdentity) -> String {
    identity
        .name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| {
            identity
                .email
                .as_deref()
                .and_then(|e| e.split('@').next())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| identity.subject.to_string())
}

#[derive(Default)]
pub struct MemoryStores {
    solving: Mutex<BTreeMap<String, SolveFile>>,
    agents: Mutex<BTreeMap<String, Entrant>>,
    people: Mutex<BTreeMap<String, Person>>,
    submissions: Mutex<BTreeMap<String, Submission>>,
    endorsements: Mutex<Vec<Endorsement>>,
    records: Mutex<BTreeMap<String, Record>>,
    sequences: Mutex<BTreeMap<i32, u64>>,
    blobs: Mutex<BTreeMap<String, Vec<u8>>>,
    queue: Mutex<BTreeSet<(String, String)>>,
    tasks: Mutex<BTreeMap<String, Task>>,
    contributions: Mutex<BTreeMap<String, Contribution>>,
    judgements: Mutex<Vec<ClaimJudgement>>,
    grants: Mutex<BTreeMap<PersonId, DonationGrant>>,
    referees: Mutex<BTreeMap<SubmissionId, RefereeFile>>,
}

impl MemoryStores {
    pub fn ports(self: &Arc<Self>, clock: Arc<dyn Clock>, reader: Arc<dyn PaperReader>) -> Ports {
        Ports {
            solving: self.clone(),
            verifier: Arc::new(UnavailableVerifier),
            clock,
            people: self.clone(),
            submissions: self.clone(),
            endorsements: self.clone(),
            records: self.clone(),
            blobs: self.clone(),
            reader,
            queue: self.clone(),
            tasks: self.clone(),
            contributions: self.clone(),
            judgements: self.clone(),
            grants: self.clone(),
            referees: self.clone(),
        }
    }

    /// Queued jobs, for tests.
    pub async fn queued(&self) -> Vec<(SubmissionId, JobKind)> {
        self.queue
            .lock()
            .await
            .iter()
            .filter_map(|(s, k)| {
                serde_json::from_str(k)
                    .ok()
                    .map(|k| (SubmissionId(s.clone()), k))
            })
            .collect()
    }

    pub async fn take_job(&self) -> Option<ReviewJob> {
        let mut queue = self.queue.lock().await;
        let first = queue.iter().next().cloned()?;
        queue.remove(&first);
        let kind = serde_json::from_str(&first.1).ok()?;
        Some(ReviewJob {
            submission: SubmissionId(first.0),
            kind,
            attempts: 0,
        })
    }
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[async_trait]
impl PersonStore for MemoryStores {
    async fn upsert_sign_in(
        &self,
        identity: &VerifiedIdentity,
        at: DateTime<Utc>,
    ) -> CoreResult<Person> {
        let mut people = self.people.lock().await;
        let person = people
            .entry(identity.subject.0.clone())
            .or_insert_with(|| Person {
                id: identity.subject.clone(),
                display_name: String::new(),
                email: None,
                picture: None,
                orcid: None,
                affiliation: None,
                roles: BTreeSet::new(),
                created_at: at,
                last_seen_at: at,
            });
        person.display_name = display_name(identity);
        person.email = identity.email.clone();
        person.picture = identity.picture.clone();
        person.last_seen_at = at;
        Ok(person.clone())
    }

    async fn get(&self, id: &PersonId) -> CoreResult<Option<Person>> {
        Ok(self.people.lock().await.get(id.as_str()).cloned())
    }

    async fn list(&self, limit: u32, before: Option<String>) -> CoreResult<Listing<Person>> {
        Ok(page(&*self.people.lock().await, limit, before, |_| true))
    }

    async fn set_roles(&self, id: &PersonId, roles: &[Role]) -> CoreResult<Person> {
        let mut people = self.people.lock().await;
        let person = people
            .get_mut(id.as_str())
            .ok_or_else(|| CoreError::not_found("person", id.as_str()))?;
        person.roles = roles.iter().copied().collect();
        Ok(person.clone())
    }
}

macro_rules! replace_with_revision {
    ($map:expr, $key:expr, $item:expr, $expected:expr, $kind:literal) => {{
        let mut map = $map.lock().await;
        let stored = map
            .get_mut($key)
            .ok_or_else(|| CoreError::not_found($kind, $key.to_string()))?;
        if stored.revision != $expected {
            return Err(CoreError::StaleRevision {
                kind: $kind,
                id: $key.to_string(),
            });
        }
        *stored = $item.clone();
        stored.revision = $expected + 1;
        Ok(())
    }};
}

#[async_trait]
impl SubmissionStore for MemoryStores {
    async fn insert(&self, submission: &Submission) -> CoreResult<()> {
        self.submissions
            .lock()
            .await
            .insert(submission.id.0.clone(), submission.clone());
        Ok(())
    }

    async fn get(&self, id: &SubmissionId) -> CoreResult<Option<Submission>> {
        Ok(self.submissions.lock().await.get(id.as_str()).cloned())
    }

    async fn list(
        &self,
        filter: &SubmissionFilter,
        limit: u32,
        before: Option<String>,
    ) -> CoreResult<Listing<Submission>> {
        Ok(page(&*self.submissions.lock().await, limit, before, |s| {
            filter.involving.as_ref().is_none_or(|p| s.involves(p))
                && filter
                    .status
                    .as_deref()
                    .is_none_or(|st| s.status.name() == st)
        }))
    }

    async fn replace(&self, submission: &Submission, expected: u64) -> CoreResult<()> {
        replace_with_revision!(
            self.submissions,
            submission.id.as_str(),
            submission,
            expected,
            "submission"
        )
    }

    async fn active_count(&self, person: &PersonId) -> CoreResult<u32> {
        Ok(self
            .submissions
            .lock()
            .await
            .values()
            .filter(|s| &s.submitter == person && s.status.is_active())
            .count() as u32)
    }
}

#[async_trait]
impl EndorsementStore for MemoryStores {
    async fn insert(&self, endorsement: &Endorsement) -> CoreResult<()> {
        let mut all = self.endorsements.lock().await;
        if all
            .iter()
            .any(|e| e.submission == endorsement.submission && e.endorser == endorsement.endorser)
        {
            return Err(CoreError::conflict(
                "this endorser has already endorsed the paper",
            ));
        }
        all.push(endorsement.clone());
        Ok(())
    }

    async fn for_submission(&self, submission: &SubmissionId) -> CoreResult<Vec<Endorsement>> {
        Ok(self
            .endorsements
            .lock()
            .await
            .iter()
            .filter(|e| &e.submission == submission)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl RecordStore for MemoryStores {
    async fn for_submission(&self, id: &SubmissionId) -> CoreResult<Option<Record>> {
        Ok(self
            .records
            .lock()
            .await
            .values()
            .find(|r| &r.submission == id)
            .cloned())
    }
    async fn next_sequence(&self, year: i32) -> CoreResult<u64> {
        let mut sequences = self.sequences.lock().await;
        let next = sequences.entry(year).or_insert(0);
        *next += 1;
        Ok(*next)
    }

    async fn insert(&self, record: &Record) -> CoreResult<()> {
        let mut records = self.records.lock().await;
        if records.contains_key(record.id.as_str())
            || records.values().any(|r| r.submission == record.submission)
        {
            return Err(CoreError::conflict(format!("record {} exists", record.id)));
        }
        records.insert(record.id.0.clone(), record.clone());
        Ok(())
    }

    async fn get(&self, id: &RecordId) -> CoreResult<Option<Record>> {
        Ok(self.records.lock().await.get(id.as_str()).cloned())
    }

    async fn list(&self, limit: u32, before: Option<String>) -> CoreResult<Listing<Record>> {
        Ok(page(&*self.records.lock().await, limit, before, |_| true))
    }
}

#[async_trait]
impl BlobStore for MemoryStores {
    async fn put(&self, bytes: &[u8], _content_type: &str) -> CoreResult<BlobRef> {
        let sha256 = sha256_hex(bytes);
        let id = crate::ids::new_uuid();
        self.blobs.lock().await.insert(id.clone(), bytes.to_vec());
        Ok(BlobRef {
            id,
            bytes: bytes.len() as u64,
            sha256,
        })
    }

    async fn get(&self, id: &str) -> CoreResult<Option<Vec<u8>>> {
        Ok(self.blobs.lock().await.get(id).cloned())
    }
}

#[async_trait]
impl ReviewQueue for MemoryStores {
    async fn enqueue(&self, submission: &SubmissionId, kind: JobKind) -> CoreResult<()> {
        let kind =
            serde_json::to_string(&kind).map_err(|e| CoreError::Unavailable(e.to_string()))?;
        self.queue.lock().await.insert((submission.0.clone(), kind));
        Ok(())
    }
}

fn task_matches(task: &Task, filter: &TaskFilter) -> bool {
    filter.kind.is_none_or(|k| task.kind == k)
        && filter
            .status
            .as_deref()
            .is_none_or(|s| task.status.name() == s)
        && filter
            .submission
            .as_ref()
            .is_none_or(|s| &task.target.submission == s)
        && filter.holder.as_ref().is_none_or(
            |h| matches!(&task.status, TaskStatus::Leased { lease } if &lease.holder == h),
        )
}

pub(crate) fn contribution_state(status: &ContributionStatus) -> &'static str {
    match status {
        ContributionStatus::Submitted => "submitted",
        ContributionStatus::Verified { .. } => "verified",
        ContributionStatus::Rejected { .. } => "rejected",
    }
}

#[async_trait]
impl TaskStore for MemoryStores {
    async fn insert_if_absent(&self, task: &Task) -> CoreResult<bool> {
        let mut tasks = self.tasks.lock().await;
        if tasks.values().any(|t| t.dedupe_key == task.dedupe_key) {
            return Ok(false);
        }
        tasks.insert(task.id.clone(), task.clone());
        Ok(true)
    }

    async fn get(&self, id: &str) -> CoreResult<Option<Task>> {
        Ok(self.tasks.lock().await.get(id).cloned())
    }

    async fn list(
        &self,
        filter: &TaskFilter,
        limit: u32,
        before: Option<String>,
    ) -> CoreResult<Listing<Task>> {
        Ok(page(&*self.tasks.lock().await, limit, before, |t| {
            task_matches(t, filter)
        }))
    }

    async fn replace(&self, task: &Task, expected: u64) -> CoreResult<()> {
        replace_with_revision!(self.tasks, task.id.as_str(), task, expected, "task")
    }

    async fn active_leases(&self, holder: &PersonId, now: DateTime<Utc>) -> CoreResult<u32> {
        Ok(self
            .tasks
            .lock()
            .await
            .values()
            .filter(|t| matches!(&t.status, TaskStatus::Leased { lease } if &lease.holder == holder && lease.until > now))
            .count() as u32)
    }

    async fn expired_leases(&self, now: DateTime<Utc>, limit: u32) -> CoreResult<Vec<Task>> {
        Ok(self
            .tasks
            .lock()
            .await
            .values()
            .filter(|t| matches!(&t.status, TaskStatus::Leased { lease } if lease.until <= now))
            .take(limit as usize)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl ContributionStore for MemoryStores {
    async fn insert(&self, contribution: &Contribution) -> CoreResult<()> {
        self.contributions
            .lock()
            .await
            .insert(contribution.id.clone(), contribution.clone());
        Ok(())
    }

    async fn get(&self, id: &str) -> CoreResult<Option<Contribution>> {
        Ok(self.contributions.lock().await.get(id).cloned())
    }

    async fn list(
        &self,
        filter: &ContributionFilter,
        limit: u32,
        before: Option<String>,
    ) -> CoreResult<Listing<Contribution>> {
        Ok(page(
            &*self.contributions.lock().await,
            limit,
            before,
            |c| {
                filter.task.as_ref().is_none_or(|t| &c.task == t)
                    && filter
                        .contributor
                        .as_ref()
                        .is_none_or(|p| &c.contributor == p)
                    && filter
                        .status
                        .as_deref()
                        .is_none_or(|s| contribution_state(&c.status) == s)
                    && filter.kind.is_none_or(|k| c.kind == k)
            },
        ))
    }

    async fn replace(&self, contribution: &Contribution, expected: u64) -> CoreResult<()> {
        replace_with_revision!(
            self.contributions,
            contribution.id.as_str(),
            contribution,
            expected,
            "contribution"
        )
    }

    async fn all_by(&self, contributor: Option<&PersonId>) -> CoreResult<Vec<Contribution>> {
        Ok(self
            .contributions
            .lock()
            .await
            .values()
            .filter(|c| contributor.is_none_or(|p| &c.contributor == p))
            .cloned()
            .collect())
    }
}

#[async_trait]
impl JudgementStore for MemoryStores {
    async fn insert(&self, judgement: &ClaimJudgement) -> CoreResult<()> {
        self.judgements.lock().await.push(judgement.clone());
        Ok(())
    }

    async fn for_submission(&self, submission: &SubmissionId) -> CoreResult<Vec<ClaimJudgement>> {
        Ok(self
            .judgements
            .lock()
            .await
            .iter()
            .filter(|j| &j.submission == submission)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl RefereeStore for MemoryStores {
    async fn get(&self, submission: &SubmissionId) -> CoreResult<Option<RefereeFile>> {
        Ok(self.referees.lock().await.get(submission).cloned())
    }

    async fn insert(&self, file: &RefereeFile) -> CoreResult<()> {
        let mut files = self.referees.lock().await;
        if files.contains_key(&file.id) {
            return Err(CoreError::conflict("the paper already has a referee file"));
        }
        files.insert(file.id.clone(), file.clone());
        Ok(())
    }

    async fn replace(&self, file: &RefereeFile, expected: u64) -> CoreResult<()> {
        let mut files = self.referees.lock().await;
        let stored = files
            .get_mut(&file.id)
            .ok_or_else(|| CoreError::not_found("referee file", file.id.as_str()))?;
        if stored.revision != expected {
            return Err(CoreError::StaleRevision {
                kind: "referee file",
                id: file.id.to_string(),
            });
        }
        *stored = file.clone();
        stored.revision = expected + 1;
        Ok(())
    }
}

#[async_trait]
impl GrantStore for MemoryStores {
    async fn get(&self, donor: &PersonId) -> CoreResult<Option<DonationGrant>> {
        Ok(self.grants.lock().await.get(donor).cloned())
    }

    async fn insert(&self, grant: &DonationGrant) -> CoreResult<()> {
        let mut grants = self.grants.lock().await;
        if grants.contains_key(&grant.donor) {
            return Err(CoreError::conflict("a grant exists for this donor"));
        }
        grants.insert(grant.donor.clone(), grant.clone());
        Ok(())
    }

    async fn replace(&self, grant: &DonationGrant, expected: u64) -> CoreResult<()> {
        let mut grants = self.grants.lock().await;
        let stored = grants
            .get_mut(&grant.donor)
            .ok_or_else(|| CoreError::not_found("grant", grant.donor.as_str()))?;
        if stored.revision != expected {
            return Err(CoreError::StaleRevision {
                kind: "grant",
                id: grant.donor.to_string(),
            });
        }
        *stored = grant.clone();
        stored.revision = expected + 1;
        Ok(())
    }

    async fn active(&self) -> CoreResult<Vec<DonationGrant>> {
        Ok(self
            .grants
            .lock()
            .await
            .values()
            .filter(|g| g.status == GrantStatus::Active)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl SolvingStore for MemoryStores {
    async fn get(&self, id: &str) -> CoreResult<Option<SolveFile>> {
        Ok(self.solving.lock().await.get(id).cloned())
    }
    async fn all(&self) -> CoreResult<Vec<SolveFile>> {
        Ok(self.solving.lock().await.values().cloned().collect())
    }
    async fn insert(&self, file: &SolveFile) -> CoreResult<()> {
        let mut files = self.solving.lock().await;
        if files.contains_key(&file.id) {
            return Err(CoreError::conflict("solve file exists"));
        }
        files.insert(file.id.clone(), file.clone());
        Ok(())
    }
    async fn replace(&self, file: &SolveFile, expected: u64) -> CoreResult<()> {
        let mut files = self.solving.lock().await;
        let stored = files
            .get_mut(&file.id)
            .ok_or_else(|| CoreError::not_found("solve file", &file.id))?;
        if stored.revision != expected {
            return Err(CoreError::StaleRevision {
                kind: "solve file",
                id: file.id.clone(),
            });
        }
        *stored = file.clone();
        stored.revision = expected + 1;
        Ok(())
    }
    async fn agents(&self) -> CoreResult<Vec<Entrant>> {
        Ok(self.agents.lock().await.values().cloned().collect())
    }
    async fn insert_agent(&self, agent: &Entrant) -> CoreResult<()> {
        let mut agents = self.agents.lock().await;
        if agents.values().any(|a| a.name == agent.name) {
            return Err(CoreError::conflict("agent name exists"));
        }
        agents.insert(agent.id.clone(), agent.clone());
        Ok(())
    }
    async fn replace_agent(&self, agent: &Entrant, expected: u64) -> CoreResult<()> {
        let mut agents = self.agents.lock().await;
        if agents
            .values()
            .any(|a| a.id != agent.id && a.name == agent.name)
        {
            return Err(CoreError::conflict("agent name exists"));
        }
        let stored = agents
            .get_mut(&agent.id)
            .ok_or_else(|| CoreError::not_found("agent", &agent.id))?;
        if stored.revision != expected {
            return Err(CoreError::StaleRevision {
                kind: "agent",
                id: agent.id.clone(),
            });
        }
        *stored = agent.clone();
        stored.revision = expected + 1;
        Ok(())
    }
}
