//! Layer-3 verifier adapter: subprocess locally, isolated HTTP deployment in infra.
use async_trait::async_trait;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wishpool_core::{
    CoreError, CoreResult,
    model::{VerificationReceipt, VerificationRequest},
    ports::Verifier,
};
pub struct ProcessVerifier {
    pub program: PathBuf,
    pub workspace: PathBuf,
    pub scratch: PathBuf,
    pub timeout: Duration,
}
#[async_trait]
impl Verifier for ProcessVerifier {
    async fn verify(&self, request: &VerificationRequest) -> CoreResult<VerificationReceipt> {
        let check = async {
            let mut child = tokio::process::Command::new(&self.program)
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap_or_default())
                .env("HOME", std::env::var_os("HOME").unwrap_or_default())
                .env("TMPDIR", &self.scratch)
                .env("WISHPOOL_LEAN_WORKSPACE", &self.workspace)
                .env(
                    "WISHPOOL_VERIFIER_TIMEOUT_SECS",
                    self.timeout.as_secs().to_string(),
                )
                .current_dir(&self.scratch)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| CoreError::Unavailable(format!("verifier: {e}")))?;
            let input =
                serde_json::to_vec(request).map_err(|e| CoreError::Unavailable(e.to_string()))?;
            let mut stdin = child.stdin.take().unwrap();
            let output = child.stdout.take().unwrap();
            let write = async {
                stdin.write_all(&input).await?;
                stdin.shutdown().await
            };
            let read = async {
                let mut bytes = vec![];
                output.take(65537).read_to_end(&mut bytes).await?;
                Ok::<_, std::io::Error>(bytes)
            };
            let (_, bytes) =
                tokio::try_join!(write, read).map_err(|e| CoreError::Unavailable(e.to_string()))?;
            if bytes.len() > 65536 {
                return Err(CoreError::Unavailable(
                    "verifier output exceeds limit".into(),
                ));
            }
            if !child
                .wait()
                .await
                .map_err(|e| CoreError::Unavailable(e.to_string()))?
                .success()
            {
                return Err(CoreError::Unavailable("verifier process failed".into()));
            }
            serde_json::from_slice(&bytes)
                .map_err(|e| CoreError::Unavailable(format!("verifier receipt: {e}")))
        };
        tokio::time::timeout(self.timeout + Duration::from_secs(10), check)
            .await
            .map_err(|_| CoreError::Unavailable("verifier deadline exceeded".into()))?
    }
}
pub struct HttpVerifier {
    pub url: String,
    pub timeout: Duration,
}
#[async_trait]
impl Verifier for HttpVerifier {
    async fn verify(&self, request: &VerificationRequest) -> CoreResult<VerificationReceipt> {
        let response = reqwest::Client::builder()
            .timeout(self.timeout + Duration::from_secs(10))
            .build()
            .map_err(|e| CoreError::Unavailable(e.to_string()))?
            .post(format!("{}/verify", self.url.trim_end_matches('/')))
            .json(request)
            .send()
            .await
            .map_err(|e| CoreError::Unavailable(e.to_string()))?
            .error_for_status()
            .map_err(|e| CoreError::Unavailable(e.to_string()))?;
        let mut response = response;
        let mut bytes = vec![];
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| CoreError::Unavailable(e.to_string()))?
        {
            if bytes.len() + chunk.len() > 65536 {
                return Err(CoreError::Unavailable(
                    "verifier receipt exceeds limit".into(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|e| CoreError::Unavailable(e.to_string()))
    }
}
