use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
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
        vec![installed_dir(), hf_snapshot_dir()]
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

    pub fn all() -> [&'static FileSpec; 2] {
        [&Self::TEXT, &Self::MEDIA]
    }

    pub const TOTAL_BYTES: u64 = Self::TEXT.bytes + Self::MEDIA.bytes;

    pub fn hugging_face_snapshot() -> PathBuf {
        hf_snapshot_dir()
    }

    /// Files that aren't in `folder` yet.
    /// `upstream/Sources/DigUpKit/ModelFiles.swift:57`
    pub fn missing(in_folder: &Path) -> Vec<&'static FileSpec> {
        Self::all()
            .into_iter()
            .filter(|f| !in_folder.join(f.name).exists())
            .collect()
    }

    /// Hugging Face cache copy, when it's there at the right size.
    /// `upstream/Sources/DigUpKit/ModelFiles.swift:50`
    pub fn cached_copy(file: &FileSpec) -> Option<PathBuf> {
        let url = hf_snapshot_dir().join(file.name);
        let resolved = std::fs::canonicalize(&url).unwrap_or(url);
        let size = std::fs::metadata(&resolved).ok()?.len();
        (size == file.bytes).then_some(resolved)
    }

    /// SHA-256 as lowercase hex, 8 MB pieces.
    /// `upstream/Sources/DigUpKit/ModelFiles.swift:62`
    pub fn sha256(path: &Path) -> Result<String, std::io::Error> {
        let mut file = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 8 << 20];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
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

fn hf_snapshot_dir() -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_model_files_in_download_order() {
        let dir = std::env::temp_dir().join(format!("trowel-models-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let missing = ModelFiles::missing(&dir);
        assert_eq!(missing.len(), 2);
        assert_eq!(missing[0].name, ModelFiles::TEXT.name);
        assert_eq!(missing[1].name, ModelFiles::MEDIA.name);
        std::fs::write(dir.join(ModelFiles::TEXT.name), []).unwrap();
        let missing = ModelFiles::missing(&dir);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].name, ModelFiles::MEDIA.name);
        assert_eq!(ModelFiles::TOTAL_BYTES, 864_676_480);
    }

    #[test]
    fn model_file_hashes_stream_in_pieces() {
        let dir = std::env::temp_dir().join(format!("trowel-sha-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let small = dir.join("abc");
        std::fs::write(&small, b"abc").unwrap();
        assert_eq!(
            ModelFiles::sha256(&small).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let bytes: Vec<u8> = (0..(9 << 20))
            .map(|i: usize| i.wrapping_mul(31) as u8)
            .collect();
        let big = dir.join("big");
        std::fs::write(&big, &bytes).unwrap();
        let expected = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(ModelFiles::sha256(&big).unwrap(), expected);
    }

    #[test]
    fn cached_copy_from_hf_hub() {
        let dir = std::env::temp_dir().join(format!("trowel-hf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let hub = dir.join("hub");
        let snapshot = hub.join(format!(
            "models--ggml-org--embeddinggemma-2-GGUF/snapshots/{}",
            ModelFiles::REVISION
        ));
        let blobs = hub.join("models--ggml-org--embeddinggemma-2-GGUF/blobs");
        std::fs::create_dir_all(&snapshot).unwrap();
        std::fs::create_dir_all(&blobs).unwrap();
        let blob = blobs.join("2188ac1d");
        std::fs::write(&blob, []).unwrap();
        let f = std::fs::File::options().write(true).open(&blob).unwrap();
        f.set_len(ModelFiles::TEXT.bytes).unwrap();
        std::os::unix::fs::symlink(&blob, snapshot.join(ModelFiles::TEXT.name)).unwrap();
        std::env::set_var("HF_HUB_CACHE", &hub);
        assert_eq!(
            ModelFiles::cached_copy(&ModelFiles::TEXT)
                .unwrap()
                .file_name()
                .unwrap(),
            "2188ac1d"
        );
        assert!(ModelFiles::cached_copy(&ModelFiles::MEDIA).is_none());
        f.set_len(10).unwrap();
        assert!(ModelFiles::cached_copy(&ModelFiles::TEXT).is_none());
        std::env::remove_var("HF_HUB_CACHE");
    }
}
