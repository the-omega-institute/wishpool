use super::{MongoStore, from_document, unavailable};
use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::{IndexModel, bson::doc, options::IndexOptions};
use wishpool_core::{
    CoreResult,
    model::{Entrant, SolveFile},
    ports::SolvingStore,
};
const FILES: &str = "solve_files";
const AGENTS: &str = "agent_entrants";
pub(super) async fn ensure_indexes(store: &MongoStore) -> anyhow::Result<()> {
    let unique = |keys| {
        IndexModel::builder()
            .keys(keys)
            .options(IndexOptions::builder().unique(true).build())
            .build()
    };
    store
        .raw(FILES)
        .create_index(unique(doc! {"id": 1}))
        .await?;
    store
        .raw(AGENTS)
        .create_indexes([unique(doc! {"id": 1}), unique(doc! {"name": 1})])
        .await?;
    Ok(())
}
#[async_trait]
impl SolvingStore for MongoStore {
    async fn get(&self, id: &str) -> CoreResult<Option<SolveFile>> {
        self.get_by_id(FILES, id).await
    }
    async fn all(&self) -> CoreResult<Vec<SolveFile>> {
        self.raw(FILES)
            .find(doc! {})
            .await
            .map_err(unavailable)?
            .try_collect::<Vec<_>>()
            .await
            .map_err(unavailable)?
            .into_iter()
            .map(from_document)
            .collect()
    }
    async fn insert(&self, file: &SolveFile) -> CoreResult<()> {
        self.insert(FILES, file).await
    }
    async fn replace(&self, file: &SolveFile, expected: u64) -> CoreResult<()> {
        self.replace_revision(FILES, "solve file", &file.id, file, expected)
            .await
    }
    async fn agents(&self) -> CoreResult<Vec<Entrant>> {
        self.raw(AGENTS)
            .find(doc! {})
            .await
            .map_err(unavailable)?
            .try_collect::<Vec<_>>()
            .await
            .map_err(unavailable)?
            .into_iter()
            .map(from_document)
            .collect()
    }
    async fn insert_agent(&self, agent: &Entrant) -> CoreResult<()> {
        self.insert(AGENTS, agent).await
    }
    async fn replace_agent(&self, agent: &Entrant, expected: u64) -> CoreResult<()> {
        self.replace_revision(AGENTS, "agent", &agent.id, agent, expected)
            .await
    }
}
