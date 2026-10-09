//! Source archives and compiled PDFs in GridFS, addressed by a UUID.

use async_trait::async_trait;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use mongodb::bson::{Bson, doc};
use sha2::{Digest, Sha256};
use wishpool_core::{CoreError, CoreResult, ids::new_uuid, model::BlobRef, ports::BlobStore};

use super::{MongoStore, unavailable};

fn gridfs(error: impl std::fmt::Display) -> CoreError {
    CoreError::Unavailable(format!("file storage: {error}"))
}

#[async_trait]
impl BlobStore for MongoStore {
    async fn put(&self, bytes: &[u8], content_type: &str) -> CoreResult<BlobRef> {
        let id = new_uuid();
        let mut upload = self
            .bucket()
            .open_upload_stream(&id)
            .id(Bson::String(id.clone()))
            .metadata(doc! { "content_type": content_type })
            .await
            .map_err(unavailable)?;
        if let Err(error) = upload.write_all(bytes).await {
            let _ = upload.abort().await;
            return Err(gridfs(error));
        }
        upload.close().await.map_err(gridfs)?;
        Ok(BlobRef {
            id,
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        })
    }

    async fn get(&self, id: &str) -> CoreResult<Option<Vec<u8>>> {
        let mut download = match self
            .bucket()
            .open_download_stream(Bson::String(id.to_owned()))
            .await
        {
            Ok(stream) => stream,
            Err(error)
                if matches!(
                    error.kind.as_ref(),
                    mongodb::error::ErrorKind::GridFs(
                        mongodb::error::GridFsErrorKind::FileNotFound { .. }
                    )
                ) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(unavailable(error)),
        };
        let mut bytes = Vec::new();
        download.read_to_end(&mut bytes).await.map_err(gridfs)?;
        Ok(Some(bytes))
    }
}
