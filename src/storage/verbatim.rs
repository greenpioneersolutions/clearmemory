use crate::security::encryption::EncryptionProvider;
use crate::StorageError;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tracing::instrument;

/// Manages raw transcript files in the verbatim storage directory.
///
/// Files are named by their SHA-256 content hash (opaque, reveals nothing).
/// Content is encrypted before writing if encryption is enabled.
pub struct VerbatimStorage {
    active_dir: PathBuf,
    archive_dir: PathBuf,
    encryption: Arc<dyn EncryptionProvider>,
}

impl VerbatimStorage {
    pub fn new(
        active_dir: PathBuf,
        archive_dir: PathBuf,
        encryption: Arc<dyn EncryptionProvider>,
    ) -> Self {
        Self {
            active_dir,
            archive_dir,
            encryption,
        }
    }

    /// Compute the SHA-256 hash of content (used as filename and content_hash).
    pub fn content_hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let hash = hasher.finalize();
        hash.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Store content. Returns the content hash.
    /// Content is encrypted before writing to disk.
    #[instrument(skip(self, content))]
    pub async fn store(&self, content: &[u8]) -> Result<String, StorageError> {
        let hash = Self::content_hash(content);
        let path = self.active_dir.join(&hash);

        // Skip if already stored (content-addressed)
        if path.exists() {
            return Ok(hash);
        }

        let encrypted = self
            .encryption
            .encrypt_bytes(content)
            .map_err(|e| StorageError::VerbatimIo(std::io::Error::other(e.to_string())))?;

        fs::write(&path, &encrypted).await?;

        Ok(hash)
    }

    /// Read content by hash. Decrypts and verifies integrity.
    #[instrument(skip(self))]
    pub async fn read(&self, content_hash: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.active_dir.join(content_hash);

        if !path.exists() {
            // Check archive
            let archive_path = self.archive_dir.join(content_hash);
            if archive_path.exists() {
                return self.read_from_path(&archive_path, content_hash).await;
            }
            return Err(StorageError::MemoryNotFound(content_hash.to_string()));
        }

        self.read_from_path(&path, content_hash).await
    }

    async fn read_from_path(
        &self,
        path: &Path,
        expected_hash: &str,
    ) -> Result<Vec<u8>, StorageError> {
        let encrypted = fs::read(path).await?;

        let plaintext = self
            .encryption
            .decrypt_bytes(&encrypted)
            .map_err(|e| StorageError::VerbatimIo(std::io::Error::other(e.to_string())))?;

        // Verify integrity
        let actual_hash = Self::content_hash(&plaintext);
        if actual_hash != expected_hash {
            return Err(StorageError::HashMismatch {
                expected: expected_hash.to_string(),
                actual: actual_hash,
            });
        }

        Ok(plaintext)
    }

    /// Move a verbatim file from active to archive directory.
    #[instrument(skip(self))]
    pub async fn archive(&self, content_hash: &str) -> Result<(), StorageError> {
        let src = self.active_dir.join(content_hash);
        let dst = self.archive_dir.join(content_hash);

        if src.exists() {
            fs::rename(&src, &dst).await?;
        }

        Ok(())
    }

    /// Delete a verbatim file permanently (for purge operations).
    #[instrument(skip(self))]
    pub async fn delete(&self, content_hash: &str) -> Result<(), StorageError> {
        let active_path = self.active_dir.join(content_hash);
        let archive_path = self.archive_dir.join(content_hash);

        if active_path.exists() {
            fs::remove_file(&active_path).await?;
        }
        if archive_path.exists() {
            fs::remove_file(&archive_path).await?;
        }

        Ok(())
    }

    /// Check if a verbatim file exists (active or archive).
    pub async fn exists(&self, content_hash: &str) -> bool {
        self.active_dir.join(content_hash).exists() || self.archive_dir.join(content_hash).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::encryption::NoopProvider;

    fn test_storage(dir: &tempfile::TempDir) -> VerbatimStorage {
        let active = dir.path().join("verbatim");
        let archive = dir.path().join("archive");
        std::fs::create_dir_all(&active).unwrap();
        std::fs::create_dir_all(&archive).unwrap();

        VerbatimStorage::new(active, archive, Arc::new(NoopProvider))
    }

    #[tokio::test]
    async fn test_store_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let storage = test_storage(&dir);

        let content = b"Hello, Clear Memory!";
        let hash = storage.store(content).await.unwrap();

        assert!(!hash.is_empty());
        assert!(storage.exists(&hash).await);

        let read_back = storage.read(&hash).await.unwrap();
        assert_eq!(read_back, content);
    }

    #[tokio::test]
    async fn test_content_addressed_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let storage = test_storage(&dir);

        let content = b"duplicate content";
        let hash1 = storage.store(content).await.unwrap();
        let hash2 = storage.store(content).await.unwrap();

        assert_eq!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_read_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let storage = test_storage(&dir);

        let result = storage.read("nonexistent_hash").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_archive_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let storage = test_storage(&dir);

        let content = b"archivable content";
        let hash = storage.store(content).await.unwrap();

        storage.archive(&hash).await.unwrap();

        // Should still be readable from archive
        let read_back = storage.read(&hash).await.unwrap();
        assert_eq!(read_back, content);
    }

    #[tokio::test]
    async fn test_delete() {
        let dir = tempfile::tempdir().unwrap();
        let storage = test_storage(&dir);

        let content = b"deletable content";
        let hash = storage.store(content).await.unwrap();

        storage.delete(&hash).await.unwrap();
        assert!(!storage.exists(&hash).await);
    }

    #[tokio::test]
    async fn test_content_hash_deterministic() {
        let hash1 = VerbatimStorage::content_hash(b"test");
        let hash2 = VerbatimStorage::content_hash(b"test");
        assert_eq!(hash1, hash2);

        let hash3 = VerbatimStorage::content_hash(b"different");
        assert_ne!(hash1, hash3);
    }
}
