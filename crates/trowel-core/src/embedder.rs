//! Embedder protocol. `upstream/Sources/DigUpKit/Worker.swift`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::vectors::DIMENSION;

#[derive(Debug, Clone)]
pub struct WorkerInfo {
    pub model: String,
    pub revision: String,
    pub runtime: String,
    pub dtype: String,
    pub dim: usize,
    pub text_only: bool,
    pub load_seconds: Option<f64>,
}

impl WorkerInfo {
    /// `upstream/Sources/DigUpKit/Worker.swift:25`
    pub fn vector_source(&self) -> String {
        let rev: String = self.revision.chars().take(7).collect();
        format!("{}@{rev} {} {}", self.model, self.runtime, self.dtype)
    }
}

#[derive(Debug, Clone)]
pub struct Embedding {
    pub vector: Option<Vec<f32>>,
    pub error: Option<String>,
}

pub trait Embedder: Send {
    fn info(&self) -> WorkerInfo;
    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
    fn embed_images(&mut self, paths: &[PathBuf], budget: i32) -> Result<Vec<Embedding>, String>;
    fn embed_audio(&mut self, paths: &[PathBuf]) -> Result<Vec<Embedding>, String>;
    fn close(&mut self);
}

/// Deterministic unit vectors for tests. `upstream/Tests/DigUpKitTests/LongFilesTests.swift` `FakeEmbedder`.
#[derive(Clone)]
pub struct FakeEmbedder {
    texts: Arc<AtomicUsize>,
}

impl FakeEmbedder {
    pub fn new() -> Self {
        Self {
            texts: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn texts(&self) -> usize {
        self.texts.load(Ordering::Relaxed)
    }

    pub fn vector_for(text: &str) -> Vec<f32> {
        let mut state: u64 = 5381;
        for byte in text.as_bytes() {
            state = state.wrapping_mul(33).wrapping_add(u64::from(*byte));
        }
        let mut values = Vec::with_capacity(DIMENSION);
        for _ in 0..DIMENSION {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            values.push(((state % 2001) as i64 - 1000) as f32 / 1000.0);
        }
        let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut values {
                *v /= norm;
            }
        }
        values
    }
}

impl Default for FakeEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl Embedder for FakeEmbedder {
    fn info(&self) -> WorkerInfo {
        WorkerInfo {
            model: "fake".into(),
            revision: "0000000".into(),
            runtime: "test".into(),
            dtype: "f32".into(),
            dim: DIMENSION,
            text_only: false,
            load_seconds: None,
        }
    }

    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.texts.fetch_add(texts.len(), Ordering::Relaxed);
        Ok(texts.iter().map(|t| Self::vector_for(t)).collect())
    }

    fn embed_images(&mut self, paths: &[PathBuf], _budget: i32) -> Result<Vec<Embedding>, String> {
        Ok(paths
            .iter()
            .map(|p| Embedding {
                vector: Some(Self::vector_for(&p.to_string_lossy())),
                error: None,
            })
            .collect())
    }

    fn embed_audio(&mut self, paths: &[PathBuf]) -> Result<Vec<Embedding>, String> {
        self.embed_images(paths, 0)
    }

    fn close(&mut self) {}
}

/// One model for both indexes. A handle's `close` leaves it running.
/// `upstream/Sources/digup/IndexService.swift` `SharedEmbedder`.
#[derive(Clone)]
pub struct SharedEmbedder {
    make: Arc<dyn Fn() -> Result<Box<dyn Embedder>, String> + Send + Sync>,
    inner: Arc<Mutex<Option<Box<dyn Embedder>>>>,
}

impl SharedEmbedder {
    pub fn new(
        make: impl Fn() -> Result<Box<dyn Embedder>, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            make: Arc::new(make),
            inner: Arc::new(Mutex::new(None)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    fn ensure(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            *guard = Some((self.make)()?);
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        if let Ok(mut g) = self.inner.lock() {
            if let Some(mut m) = g.take() {
                m.close();
            }
        }
    }
}

impl Embedder for SharedEmbedder {
    fn info(&self) -> WorkerInfo {
        self.ensure().ok();
        self.inner
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|m| m.info()))
            .unwrap_or(WorkerInfo {
                model: String::new(),
                revision: String::new(),
                runtime: String::new(),
                dtype: String::new(),
                dim: DIMENSION,
                text_only: false,
                load_seconds: None,
            })
    }

    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.ensure()?;
        self.inner
            .lock()
            .map_err(|e| e.to_string())?
            .as_mut()
            .ok_or_else(|| "embedder closed".to_string())?
            .embed_texts(texts)
    }

    fn embed_images(&mut self, paths: &[PathBuf], budget: i32) -> Result<Vec<Embedding>, String> {
        self.ensure()?;
        self.inner
            .lock()
            .map_err(|e| e.to_string())?
            .as_mut()
            .ok_or_else(|| "embedder closed".to_string())?
            .embed_images(paths, budget)
    }

    fn embed_audio(&mut self, paths: &[PathBuf]) -> Result<Vec<Embedding>, String> {
        self.ensure()?;
        self.inner
            .lock()
            .map_err(|e| e.to_string())?
            .as_mut()
            .ok_or_else(|| "embedder closed".to_string())?
            .embed_audio(paths)
    }

    fn close(&mut self) {}
}
