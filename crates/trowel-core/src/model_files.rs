use std::env;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// ggml-org EmbeddingGemma 2 GGUF files. Names, revision and SHA-256 must
/// match `upstream/Sources/DigUpKit/ModelFiles.swift`.
pub struct ModelFiles;

pub struct FileSpec {
    pub name: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("no {0} in {1}")]
    NotFound(String, String),
}

impl ModelFiles {
    pub const REPOSITORY: &'static str = "ggml-org/embeddinggemma-2-GGUF";
    pub const REVISION: &'static str = "bfcd298762cc34d0357ece5ebdd31791a3a374d8";

    pub const TEXT: FileSpec = FileSpec {
        name: "embeddinggemma-2-Q8_0.gguf",
        bytes: 309_855_456,
        sha256: "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135",
    };

    pub const MEDIA: FileSpec = FileSpec {
        name: "mmproj-embeddinggemma-2-Q8_0.gguf",
        bytes: 554_821_024,
        sha256: "c4a8a52691ecef40618438928bdf9e68379b854e24166f292592353db0aab64f",
    };

    /// Folders to look in. `$TROWEL_MODELS` (or `$DIGUP_MODELS`) wins if set.
    pub fn folders() -> Vec<PathBuf> {
        if let Some(chosen) = env::var_os("TROWEL_MODELS").or_else(|| env::var_os("DIGUP_MODELS")) {
            if !chosen.is_empty() {
                return vec![PathBuf::from(chosen)];
            }
        }
        vec![installed_dir(), hugging_face_snapshot()]
    }

    pub fn locate_text() -> Result<PathBuf, ModelError> {
        for folder in Self::folders() {
            let text = folder.join(Self::TEXT.name);
            if text.is_file() {
                return Ok(text);
            }
        }
        Err(ModelError::NotFound(
            Self::TEXT.name.to_string(),
            Self::folders()
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ))
    }

    pub fn locate_media() -> Option<PathBuf> {
        for folder in Self::folders() {
            let media = folder.join(Self::MEDIA.name);
            if media.is_file() {
                return Some(media);
            }
        }
        None
    }

    pub fn complete() -> bool {
        Self::locate_text().is_ok() && Self::locate_media().is_some()
    }
}

fn installed_dir() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return Path::new(&xdg).join("trowel/models");
        }
    }
    dirs_home().join(".local/share/trowel/models")
}

fn hugging_face_snapshot() -> PathBuf {
    let hub = env::var_os("HF_HUB_CACHE")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HF_HOME").map(|h| PathBuf::from(h).join("hub")))
        .unwrap_or_else(|| dirs_home().join(".cache/huggingface/hub"));
    hub.join(format!(
        "models--{}/snapshots/{}",
        ModelFiles::REPOSITORY.replace('/', "--"),
        ModelFiles::REVISION
    ))
}

fn dirs_home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}
