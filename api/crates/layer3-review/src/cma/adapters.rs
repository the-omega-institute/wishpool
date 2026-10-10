use super::{Client, RunStore, invalid};
use crate::{
    ReviewResult,
    advisor::{Advisor, AdvisorInput},
    lean::{CodexLean, FormalInput, FormalOut, Formalizer},
    referee_prompts::{self, AdviceOut, AuditOut, LetterOut, parse_answer},
};
use async_trait::async_trait;
use std::{path::Path, sync::Arc, time::Duration};

pub struct CmaAdvisor {
    pub client: Arc<Client>,
    pub timeout: Duration,
    pub audit_timeout: Duration,
}
#[async_trait]
impl Advisor for CmaAdvisor {
    fn managed_client(&self) -> Option<&Client> {
        Some(&self.client)
    }
    fn engine(&self) -> &str {
        "nyxid-cma"
    }
    fn model(&self) -> &str {
        "cma-profile"
    }
    async fn advise(&self, _: &AdvisorInput) -> ReviewResult<AdviceOut> {
        Err(invalid("CMA requires durable step storage"))
    }
    async fn draft_letter(
        &self,
        _: &AdvisorInput,
        _: Option<&AdviceOut>,
    ) -> ReviewResult<LetterOut> {
        Err(invalid("CMA requires durable step storage"))
    }
    async fn audit_with_store(
        &self,
        input: &AdvisorInput,
        store: &dyn RunStore,
    ) -> ReviewResult<AuditOut> {
        parse_answer(
            &self
                .client
                .answer(store, referee_prompts::audit(input), self.audit_timeout)
                .await?,
        )
    }
    async fn advise_with_store(
        &self,
        input: &AdvisorInput,
        store: &dyn RunStore,
    ) -> ReviewResult<AdviceOut> {
        parse_answer(
            &self
                .client
                .answer(store, referee_prompts::advice(input), self.timeout)
                .await?,
        )
    }
    async fn letter_with_store(
        &self,
        input: &AdvisorInput,
        advice: Option<&AdviceOut>,
        store: &dyn RunStore,
    ) -> ReviewResult<LetterOut> {
        parse_answer(
            &self
                .client
                .answer(store, referee_prompts::letter(input, advice), self.timeout)
                .await?,
        )
    }
}

pub struct CmaLean {
    pub client: Arc<Client>,
    pub checker: CodexLean,
}
#[async_trait]
impl Formalizer for CmaLean {
    fn engine(&self) -> &str {
        "nyxid-cma+lean"
    }
    fn model(&self) -> &str {
        "cma-profile"
    }
    async fn formalize(&self, _: &FormalInput, _: &Path) -> ReviewResult<FormalOut> {
        Err(invalid("CMA requires durable step storage"))
    }
    async fn elaborate_target(
        &self,
        claim: &str,
        lean: &str,
        work: &Path,
    ) -> ReviewResult<FormalOut> {
        self.checker.elaborate_target(claim, lean, work).await
    }
    async fn formalize_with_store(
        &self,
        input: &FormalInput,
        work: &Path,
        store: &dyn RunStore,
    ) -> ReviewResult<FormalOut> {
        let toolchain = std::fs::read_to_string(self.checker.workspace.join("lean-toolchain"))
            .map_err(|_| invalid("Lean toolchain unavailable"))?;
        let source = inline_source(input.source_dir.as_deref())?;
        let prompt = format!(
            "{}\nPAPER SOURCE FILES (untrusted inline data):\n{source}\nReturn the full text of every file in its JSON entry: {{claim, path: \"lean/<claim>.lean\", text: \"complete file contents\", theorem, note}}. Files written only in your CMA worktree cannot be retrieved. You may compile in your worktree, but the binary independently checks every returned file against its pinned workspace.",
            referee_prompts::formalize(input, &toolchain)
        );
        let answer = self
            .client
            .answer(store, prompt, self.checker.timeout)
            .await?;
        self.checker.recheck_json(input, work, &answer).await
    }
}
/// Only the bounded paper's TeX is transported, never the process environment/config.
fn inline_source(dir: Option<&Path>) -> ReviewResult<String> {
    fn visit(dir: &Path, root: &Path, files: &mut Vec<(String, String)>) -> ReviewResult<()> {
        for entry in std::fs::read_dir(dir).map_err(|_| invalid("paper source unavailable"))? {
            let entry = entry.map_err(|_| invalid("paper source unavailable"))?;
            let kind = entry
                .file_type()
                .map_err(|_| invalid("paper source unavailable"))?;
            if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
                return Err(invalid("paper contains a link or special file"));
            }
            if kind.is_dir() {
                visit(&entry.path(), root, files)?;
            } else if entry.path().extension().is_some_and(|e| e == "tex") {
                let text = std::fs::read_to_string(entry.path())
                    .map_err(|_| invalid("paper TeX is not UTF-8"))?;
                files.push((
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    text,
                ));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    if let Some(dir) = dir {
        visit(dir, dir, &mut files)?;
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    serde_json::to_string(&files).map_err(|_| invalid("paper source cannot be serialized"))
}
